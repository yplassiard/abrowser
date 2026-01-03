//! Interactive shell for browsing

mod config;
mod input;
mod media;
mod render;
mod state;

pub use config::{Config, ViewportMode};
pub use media::{MediaController, MediaStatus};
pub use state::{BrowserState, FocusMode, Tab};

use config::ViewportMode as VP;

use crate::browser::BrowserLauncher;
use crate::cdp::{AccessibilityDomain, CdpClient};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal,
};
use std::io;

/// Main interactive browser shell
pub struct Shell {
    state: BrowserState,
    launcher: BrowserLauncher,
    browser_client: Option<CdpClient>,
    config: Config,
}

impl Shell {
    pub fn new(config: Config) -> Self {
        let viewport_mode = config.viewport_mode;
        let (width, height) = viewport_mode.dimensions();
        Self {
            state: BrowserState::new().with_viewport(viewport_mode),
            launcher: BrowserLauncher::new().with_viewport(width, height),
            browser_client: None,
            config,
        }
    }

    /// Run the interactive shell
    pub async fn run(&mut self, initial_url: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Launch browser
        let browser_ws_url = self.launcher.launch()?;
        self.browser_client = Some(CdpClient::connect(&browser_ws_url).await?);

        // Enter raw mode
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen)?;

        // Load initial URL or a blank page
        let url = initial_url.unwrap_or("about:blank");
        if let Err(e) = self.open_url(url).await {
            self.state.set_status(&format!("Error loading page: {}", e));
        }

        // Main event loop
        let result = self.event_loop().await;

        // Cleanup
        execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
        terminal::disable_raw_mode()?;

        result
    }

    async fn event_loop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut media_update_counter = 0u8;
        let mut refresh_counter = 0u8;

        loop {
            // Update media status less frequently (every ~500ms instead of 100ms)
            media_update_counter = media_update_counter.wrapping_add(1);
            if media_update_counter % 10 == 0 {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(30),
                    self.update_media_status()
                ).await;
            }

            // Check for CDP events (non-blocking) - handles loading progress and DOM changes
            let needs_immediate_refresh = self.process_cdp_events();

            // Check for loading timeout - force tree refresh after 1.5s if still loading with no content
            let loading_timeout = {
                let tab = self.state.current_tab();
                if let Some(started) = tab.loading_started {
                    started.elapsed() > std::time::Duration::from_millis(1500) && tab.node_count() == 0
                } else {
                    false
                }
            };

            // Refresh tree if needed (with debounce for DOM changes)
            refresh_counter = refresh_counter.wrapping_add(1);
            let dom_debounce_ready = {
                let tab = self.state.current_tab();
                if let Some(last_change) = tab.last_dom_change {
                    // Refresh 200ms after last DOM change
                    last_change.elapsed() > std::time::Duration::from_millis(200)
                } else {
                    false
                }
            };
            let should_refresh = needs_immediate_refresh
                || loading_timeout
                || (dom_debounce_ready && self.state.current_tab().needs_tree_refresh)
                || (refresh_counter % 20 == 0 && self.state.current_tab().needs_tree_refresh);

            if should_refresh {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(200),
                    self.force_refresh_tree()
                ).await;
            }

            // Render current state
            self.render()?;

            // Wait for input (shorter interval for responsiveness)
            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key).await? {
                        break; // Exit requested
                    }
                }
            }
        }
        Ok(())
    }

    /// Process any pending CDP events (non-blocking)
    fn process_cdp_events(&mut self) -> bool {
        let mut needs_refresh = false;

        // Collect events first to avoid borrow issues
        let events: Vec<_> = {
            if let Some(ref client) = self.state.current_tab().page_client {
                let mut events = Vec::new();
                while let Some(event) = client.try_recv_event() {
                    events.push(event);
                }
                events
            } else {
                Vec::new()
            }
        };

        // Process collected events
        for event in events {
            match event.method.as_str() {
                // Network events for loading progress
                "Network.requestWillBeSent" => {
                    let tab = self.state.current_tab_mut();
                    tab.pending_requests += 1;
                    tab.total_requests += 1;
                    tab.update_loading_progress();
                }
                "Network.loadingFinished" | "Network.loadingFailed" => {
                    let tab = self.state.current_tab_mut();
                    tab.pending_requests = tab.pending_requests.saturating_sub(1);
                    tab.update_loading_progress();
                }
                // DOM/Accessibility changes - trigger refresh
                "DOM.documentUpdated" => {
                    // Full document change - refresh immediately
                    self.state.current_tab_mut().needs_tree_refresh = true;
                    needs_refresh = true;
                }
                "DOM.childNodeCountUpdated" | "DOM.childNodeInserted" | "DOM.childNodeRemoved" => {
                    // Incremental DOM changes - mark for debounced refresh
                    let tab = self.state.current_tab_mut();
                    tab.needs_tree_refresh = true;
                    tab.last_dom_change = Some(std::time::Instant::now());
                }
                "Accessibility.loadComplete" | "Accessibility.nodesUpdated" => {
                    self.state.current_tab_mut().needs_tree_refresh = true;
                    needs_refresh = true;
                }
                // Page load complete
                "Page.loadEventFired" => {
                    self.state.current_tab_mut().finish_loading();
                    self.state.current_tab_mut().needs_tree_refresh = true;
                    needs_refresh = true;
                }
                "Page.domContentEventFired" => {
                    self.state.current_tab_mut().needs_tree_refresh = true;
                }
                _ => {}
            }
        }

        // Update status with loading progress if loading
        if let Some(progress) = self.state.current_tab().loading_progress {
            let pending = self.state.current_tab().pending_requests;
            self.state.set_status(&format!("Loading... {}% ({} pending)", progress, pending));
        }

        needs_refresh
    }

    /// Refresh the accessibility tree if needed
    async fn maybe_refresh_tree(&mut self) {
        if !self.state.current_tab().needs_tree_refresh {
            return;
        }
        self.force_refresh_tree().await;
    }

    /// Force refresh the accessibility tree (also clears loading state if successful)
    async fn force_refresh_tree(&mut self) {
        // Save current element for focus restoration
        let saved_element = {
            let tab = self.state.current_tab();
            tab.current_node().map(|node| {
                (node.role_str().to_string(), node.name_str().to_string())
            })
        };

        if let Some(ref client) = self.state.current_tab().page_client {
            let ax = AccessibilityDomain::new(client);
            if let Ok(tree) = ax.get_full_tree().await {
                let tab = self.state.current_tab_mut();
                let had_content = tab.node_count() > 0;
                tab.set_tree(tree);
                tab.needs_tree_refresh = false;
                tab.last_dom_change = None;

                // If we now have content, clear loading state
                if tab.node_count() > 0 {
                    tab.finish_loading();
                    if !had_content {
                        self.state.set_status("Ready");
                    }
                }

                // Restore focus if possible
                if let Some((role, name)) = saved_element {
                    if let Some(idx) = self.state.current_tab().find_by_role_and_name(&role, &name) {
                        self.state.current_tab_mut().cursor_index = idx;
                    }
                }
            }
        }
    }

    async fn update_media_status(&mut self) {
        if let Some(ref client) = self.state.current_tab().page_client {
            if let Ok(status) = MediaController::get_status(client).await {
                self.state.media_status = if status.has_video {
                    Some(status)
                } else {
                    None
                };
            }
        }
    }

    /// Focus the current element in the browser (triggers onFocus/onBlur events)
    async fn focus_current_in_browser(&mut self) {
        // Get the backend DOM node ID of the current element
        let backend_id = {
            let tab = self.state.current_tab();
            tab.current_node().and_then(|node| node.backend_dom_node_id)
        };

        if let Some(backend_id) = backend_id {
            if let Some(ref client) = self.state.current_tab().page_client {
                // Focus the element - this triggers onFocus/onBlur events in JS
                let _ = client.call("DOM.focus", serde_json::json!({
                    "backendNodeId": backend_id
                })).await;
            }
        }
    }

    /// Scroll the browser viewport by a given amount (positive = down, negative = up)
    async fn scroll_browser_viewport(&mut self, delta: i32) {
        if let Some(ref client) = self.state.current_tab().page_client {
            let _ = client.call("Runtime.evaluate", serde_json::json!({
                "expression": format!("window.scrollBy(0, {})", delta)
            })).await;
            // Mark for tree refresh to pick up newly loaded content
            self.state.current_tab_mut().needs_tree_refresh = true;
        }
    }

    /// Scroll browser to top of page
    async fn scroll_browser_to_top(&mut self) {
        if let Some(ref client) = self.state.current_tab().page_client {
            let _ = client.call("Runtime.evaluate", serde_json::json!({
                "expression": "window.scrollTo(0, 0)"
            })).await;
            self.state.current_tab_mut().needs_tree_refresh = true;
        }
    }

    /// Scroll browser to bottom of page (triggers lazy loading)
    async fn scroll_browser_to_bottom(&mut self) {
        if let Some(ref client) = self.state.current_tab().page_client {
            let _ = client.call("Runtime.evaluate", serde_json::json!({
                "expression": "window.scrollTo(0, document.body.scrollHeight)"
            })).await;
            self.state.current_tab_mut().needs_tree_refresh = true;
        }
    }

    /// Handle a key event, returns true if should exit
    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Check for Ctrl+Q quit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            return Ok(true);
        }

        // Handle based on focus mode
        match self.state.focus_mode {
            FocusMode::Navigation => self.handle_navigation_key(key).await,
            FocusMode::Focus => self.handle_focus_key(key).await,
        }
    }

    async fn handle_navigation_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        match (key.modifiers, key.code) {
            // Ctrl+ shortcuts
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                self.open_address_bar().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                self.new_tab().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                self.close_current_tab().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
                self.open_file_dialog().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                self.save_page().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
                self.show_downloads().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                self.print_page().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                self.refresh_page().await?;
            }
            (KeyModifiers::NONE, KeyCode::F(5)) => {
                self.refresh_page().await?;
            }
            (KeyModifiers::NONE, KeyCode::F(4)) => {
                // Quick tree refresh without page reload
                self.state.set_status("Refreshing tree...");
                self.force_refresh_tree().await;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
                self.toggle_viewport().await?;
            }
            (KeyModifiers::CONTROL, KeyCode::Char(' ')) => {
                self.state.toggle_focus_mode();
            }
            (KeyModifiers::ALT, KeyCode::Char(c)) if c.is_ascii_digit() => {
                // Alt+0-9 to switch tabs (Ctrl+number doesn't work in terminals)
                let tab_num = c.to_digit(10).unwrap() as usize;
                self.switch_tab(tab_num)?;
            }
            (KeyModifiers::CONTROL, KeyCode::Home) => {
                self.state.cursor_to_top();
                self.scroll_browser_to_top().await;
            }
            (KeyModifiers::CONTROL, KeyCode::End) => {
                self.state.cursor_to_bottom();
                self.scroll_browser_to_bottom().await;
            }

            // Navigation keys
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.state.cursor_up();
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.state.cursor_down();
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Home) => {
                self.state.cursor_to_line_start();
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::End) => {
                self.state.cursor_to_line_end();
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.state.page_up();
                self.scroll_browser_viewport(-500).await;
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                self.state.page_down();
                self.scroll_browser_viewport(500).await;
                self.focus_current_in_browser().await;
            }
            // History navigation (left/right arrows)
            (KeyModifiers::NONE, KeyCode::Left) => {
                self.navigate_back().await?;
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                self.navigate_forward().await?;
            }
            // History menu
            (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
                self.show_history_menu().await?;
            }

            // Element navigation (lowercase = next, uppercase = previous)
            (KeyModifiers::NONE, KeyCode::Char('h')) => {
                self.state.next_element("heading");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('H')) => {
                self.state.prev_element("heading");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) => {
                self.state.next_element("link");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('K')) => {
                self.state.prev_element("link");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('b')) => {
                self.state.next_element("button");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('B')) => {
                self.state.prev_element("button");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('e')) => {
                self.state.next_element("textbox");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('E')) => {
                self.state.prev_element("textbox");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('x')) => {
                self.state.next_element("checkbox");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('X')) => {
                self.state.prev_element("checkbox");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                self.state.next_element("radiobutton");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('R')) => {
                self.state.prev_element("radiobutton");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('l')) => {
                self.state.next_element("list");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('L')) => {
                self.state.prev_element("list");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('t')) => {
                self.state.next_element("table");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('T')) => {
                self.state.prev_element("table");
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::NONE, KeyCode::Char('v')) => {
                self.state.next_visited_link();
                self.focus_current_in_browser().await;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('V')) => {
                self.state.prev_visited_link();
                self.focus_current_in_browser().await;
            }

            // Text search (vim-style)
            (_, KeyCode::Char('/')) => {
                self.search_prompt(true).await?;
            }
            (_, KeyCode::Char('?')) => {
                self.search_prompt(false).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('s')) => {
                // Search next
                if self.state.search_next() {
                    self.focus_current_in_browser().await;
                    self.state.set_status(&format!("/{}", self.state.search_pattern));
                } else if !self.state.search_pattern.is_empty() {
                    self.state.set_status("Pattern not found");
                }
            }
            (KeyModifiers::SHIFT, KeyCode::Char('S')) => {
                // Search previous
                if self.state.search_prev() {
                    self.focus_current_in_browser().await;
                    self.state.set_status(&format!("?{}", self.state.search_pattern));
                } else if !self.state.search_pattern.is_empty() {
                    self.state.set_status("Pattern not found");
                }
            }

            // Tab navigation through focusable elements (uses page's tab order)
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.tab_to_next_element(false).await?;
            }
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.tab_to_next_element(true).await?;
            }

            // Actions
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.activate_current_element().await?;
            }
            (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                // Space: play/pause if video, otherwise toggle element
                if !self.try_media_play_pause().await? {
                    self.toggle_current_element().await?;
                }
            }

            // Media controls (YouTube-style)
            (KeyModifiers::NONE, KeyCode::Char('p')) => {
                // 'p' for play/pause (alternative to space)
                self.try_media_play_pause().await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('m')) => {
                self.media_toggle_mute().await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('j')) => {
                // Rewind 10 seconds
                self.media_seek(-10.0).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char(';')) => {
                // Forward 10 seconds
                self.media_seek(10.0).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char(',')) => {
                // Rewind 5 seconds
                self.media_seek(-5.0).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('.')) => {
                // Forward 5 seconds
                self.media_seek(5.0).await?;
            }
            (_, KeyCode::Char('<')) => {
                // Previous tab
                self.prev_tab();
            }
            (_, KeyCode::Char('>')) => {
                // Next tab
                self.next_tab();
            }
            (KeyModifiers::NONE, KeyCode::Char('[')) => {
                // Slower playback
                self.media_adjust_speed(-0.25).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char(']')) => {
                // Faster playback
                self.media_adjust_speed(0.25).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('c')) => {
                // Toggle captions
                self.media_toggle_captions().await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('0')) => {
                self.media_seek_percent(0).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('1')) => {
                self.media_seek_percent(10).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('2')) => {
                self.media_seek_percent(20).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('3')) => {
                self.media_seek_percent(30).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('4')) => {
                self.media_seek_percent(40).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('5')) => {
                self.media_seek_percent(50).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('6')) => {
                self.media_seek_percent(60).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('7')) => {
                self.media_seek_percent(70).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('8')) => {
                self.media_seek_percent(80).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('9')) => {
                self.media_seek_percent(90).await?;
            }
            (KeyModifiers::SHIFT, KeyCode::Char('+')) | (KeyModifiers::NONE, KeyCode::Char('=')) => {
                // Volume up
                self.media_adjust_volume(0.1).await?;
            }
            (KeyModifiers::NONE, KeyCode::Char('-')) => {
                // Volume down
                self.media_adjust_volume(-0.1).await?;
            }

            _ => {}
        }

        Ok(false)
    }

    async fn handle_focus_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Ctrl+Space or Escape toggles back to navigation
        if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(' '))
            || key.code == KeyCode::Esc
        {
            self.state.toggle_focus_mode();
            self.state.set_status("Navigation mode");
            return Ok(false);
        }

        // Ctrl+Q quits even in focus mode
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
            return Ok(true);
        }

        // Pass other keys through to the focused element
        self.send_key_to_element(key).await?;
        Ok(false)
    }

    /// Open a URL in the current tab
    pub async fn open_url(&mut self, url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let browser_client = self.browser_client.as_ref().ok_or("Browser not connected")?;

        // Normalize URL
        let url = if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://") {
            url.to_string()
        } else if url.contains('.') {
            format!("https://{}", url)
        } else {
            // Search query
            format!("https://duckduckgo.com/?q={}", urlencoding::encode(url))
        };

        // Start loading state
        self.state.current_tab_mut().start_loading();
        self.state.set_status(&format!("Loading: {}", url));
        self.render()?;

        // Create page target
        let page_ws_url = self.launcher.create_page(browser_client, &url).await?;
        let page_client = CdpClient::connect(&page_ws_url).await?;

        // Enable domains for event subscriptions (non-blocking)
        let _ = page_client.call("Network.enable", serde_json::json!({})).await;
        let _ = page_client.call("Page.enable", serde_json::json!({})).await;
        let _ = page_client.call("DOM.enable", serde_json::json!({})).await;

        // Enable accessibility
        let accessibility = AccessibilityDomain::new(&page_client);
        let _ = accessibility.enable().await;

        // Store client immediately so user can interact
        let tab = self.state.current_tab_mut();
        tab.url = url;
        tab.page_client = Some(page_client);
        tab.needs_tree_refresh = true;
        tab.push_history();
        tab.loading_started = Some(std::time::Instant::now());

        // Loading continues in background - event loop will handle tree refresh
        self.state.set_status("Loading...");
        Ok(())
    }

    /// Navigate back in history
    async fn navigate_back(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(url) = self.state.current_tab_mut().go_back() {
            self.state.set_status("Going back...");
            // Navigate to the URL from history (don't push to history again)
            self.navigate_to_history_url(&url).await?;
        } else {
            self.state.set_status("No history to go back");
        }
        Ok(())
    }

    /// Navigate forward in history
    async fn navigate_forward(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(url) = self.state.current_tab_mut().go_forward() {
            self.state.set_status("Going forward...");
            self.navigate_to_history_url(&url).await?;
        } else {
            self.state.set_status("No history to go forward");
        }
        Ok(())
    }

    /// Navigate to a URL from history (without adding to history)
    async fn navigate_to_history_url(&mut self, url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let browser_client = self.browser_client.as_ref().ok_or("Browser not connected")?;

        self.state.current_tab_mut().start_loading();
        self.render()?;

        let page_ws_url = self.launcher.create_page(browser_client, url).await?;
        let page_client = CdpClient::connect(&page_ws_url).await?;

        let _ = page_client.call("Network.enable", serde_json::json!({})).await;
        let _ = page_client.call("Page.enable", serde_json::json!({})).await;
        let _ = page_client.call("DOM.enable", serde_json::json!({})).await;

        let accessibility = AccessibilityDomain::new(&page_client);
        let _ = accessibility.enable().await;

        let tab = self.state.current_tab_mut();
        tab.url = url.to_string();
        tab.page_client = Some(page_client);
        tab.needs_tree_refresh = true;
        // Don't push to history - we're navigating within history

        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.maybe_refresh_tree()
        ).await;

        self.state.set_status("Loading...");
        Ok(())
    }

    /// Show history menu for current tab
    async fn show_history_menu(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let history = self.state.current_tab().history.clone();
        let current_index = self.state.current_tab().history_index;

        if history.is_empty() {
            self.state.set_status("No history");
            return Ok(());
        }

        // Show history in input prompt area
        self.state.show_input_prompt("History (↑/↓ select, Enter go, Esc cancel): ");
        let mut selected = current_index;
        self.render_history_menu(&history, selected)?;

        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up => {
                        if selected > 0 {
                            selected -= 1;
                            self.render_history_menu(&history, selected)?;
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < history.len() {
                            selected += 1;
                            self.render_history_menu(&history, selected)?;
                        }
                    }
                    KeyCode::Enter => {
                        self.state.hide_input_prompt();
                        if selected != current_index {
                            // Navigate to selected history entry
                            self.state.current_tab_mut().history_index = selected;
                            let url = history[selected].url.clone();
                            self.navigate_to_history_url(&url).await?;
                        }
                        break;
                    }
                    KeyCode::Esc => {
                        self.state.hide_input_prompt();
                        break;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn render_history_menu(&mut self, history: &[state::HistoryEntry], selected: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Build menu string showing history entries
        let mut menu = String::new();
        let start = selected.saturating_sub(3);
        let end = (start + 7).min(history.len());

        for (i, entry) in history.iter().enumerate().skip(start).take(end - start) {
            let marker = if i == selected { ">" } else { " " };
            let title = if entry.title.is_empty() { &entry.url } else { &entry.title };
            let truncated = if title.len() > 50 {
                format!("{}...", &title[..47])
            } else {
                title.to_string()
            };
            if !menu.is_empty() {
                menu.push_str(" | ");
            }
            menu.push_str(&format!("{}{}", marker, truncated));
        }

        self.state.set_input_value(&menu);
        self.render()?;
        Ok(())
    }

    async fn open_address_bar(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Pre-fill with current URL
        let current_url = self.state.current_tab().url.clone();
        let mut input = current_url.clone();

        // Show address bar prompt with current URL
        self.state.show_input_prompt("URL: ");
        self.state.set_input_value(&input);
        self.render()?;

        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        self.state.hide_input_prompt();
                        if !input.is_empty() {
                            self.open_url(&input).await?;
                        }
                        break;
                    }
                    KeyCode::Esc => {
                        self.state.hide_input_prompt();
                        break;
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+U: clear input
                        input.clear();
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+W: delete word backwards
                        while input.ends_with(char::is_whitespace) {
                            input.pop();
                        }
                        while !input.is_empty() && !input.ends_with(char::is_whitespace) {
                            input.pop();
                        }
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn search_prompt(&mut self, forward: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let prompt = if forward { "/" } else { "?" };
        let mut input = String::new();

        self.state.show_input_prompt(prompt);
        self.state.set_input_value(&input);
        self.render()?;

        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        self.state.hide_input_prompt();
                        if !input.is_empty() {
                            self.state.search_pattern = input;
                            self.state.search_forward = forward;
                            if self.state.search_next() {
                                self.focus_current_in_browser().await;
                                let pattern = self.state.search_pattern.clone();
                                self.state.set_status(&format!("{}{}", prompt, pattern));
                            } else {
                                self.state.set_status("Pattern not found");
                            }
                        }
                        break;
                    }
                    KeyCode::Esc => {
                        self.state.hide_input_prompt();
                        break;
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input.clear();
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        while input.ends_with(char::is_whitespace) {
                            input.pop();
                        }
                        while !input.is_empty() && !input.ends_with(char::is_whitespace) {
                            input.pop();
                        }
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Edit a text field with a prompt showing the field's label
    async fn edit_text_field(&mut self, label: &str, backend_id: i64, role: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let is_textarea = role == "textarea";
        let is_combobox = role == "combobox";

        // Get current value and options (for combobox) from the field
        let (current_value, options) = if let Some(ref client) = self.state.current_tab().page_client {
            // First focus the element
            let _ = client.call("DOM.focus", serde_json::json!({
                "backendNodeId": backend_id
            })).await;

            // Try to get the current value via JavaScript
            let result = client.call("Runtime.evaluate", serde_json::json!({
                "expression": "document.activeElement?.value || ''"
            })).await;

            let value = result.ok()
                .and_then(|r| r.get("result").cloned())
                .and_then(|r| r.get("value").cloned())
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            // For combobox, try to get options from datalist or associated list
            let opts: Vec<String> = if is_combobox {
                let opts_result = client.call("Runtime.evaluate", serde_json::json!({
                    "expression": r#"
                        (function() {
                            const el = document.activeElement;
                            if (!el) return [];
                            // Check for datalist
                            const listId = el.getAttribute('list');
                            if (listId) {
                                const datalist = document.getElementById(listId);
                                if (datalist) {
                                    return Array.from(datalist.options).map(o => o.value || o.textContent);
                                }
                            }
                            // Check for aria-owns or aria-controls
                            const listboxId = el.getAttribute('aria-owns') || el.getAttribute('aria-controls');
                            if (listboxId) {
                                const listbox = document.getElementById(listboxId);
                                if (listbox) {
                                    return Array.from(listbox.querySelectorAll('[role="option"]')).map(o => o.textContent.trim());
                                }
                            }
                            return [];
                        })()
                    "#
                })).await;

                opts_result.ok()
                    .and_then(|r| r.get("result").cloned())
                    .and_then(|r| r.get("value").cloned())
                    .and_then(|v| v.as_array().cloned())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            (value, opts)
        } else {
            (String::new(), Vec::new())
        };

        // Format the prompt - use label or a default
        let prompt_label = if label.is_empty() { "Input" } else { label };
        let prompt = if is_textarea {
            format!("{} (Tab to submit): ", prompt_label)
        } else if is_combobox && !options.is_empty() {
            format!("{} (↑↓ for options): ", prompt_label)
        } else {
            format!("{}: ", prompt_label)
        };

        let mut input = current_value;
        let mut cursor = input.len(); // Cursor position in bytes
        let mut option_index: Option<usize> = None;

        self.state.show_input_prompt(&prompt);
        self.state.set_input_value_with_cursor(&input, cursor);
        self.render()?;

        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter if !is_textarea => {
                        // Submit for non-textarea fields
                        self.state.hide_input_prompt();
                        self.submit_text_field(&input, backend_id, true).await?;
                        self.state.set_status(&format!("Set {}: {}", prompt_label, input));
                        self.state.current_tab_mut().needs_tree_refresh = true;
                        break;
                    }
                    KeyCode::Enter if is_textarea => {
                        // Insert newline for textarea
                        input.insert(cursor, '\n');
                        cursor += 1;
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Tab => {
                        // Submit and move to next element
                        self.state.hide_input_prompt();
                        self.submit_text_field(&input, backend_id, false).await?;
                        self.state.set_status(&format!("Set {}: {}", prompt_label, input));
                        self.state.current_tab_mut().needs_tree_refresh = true;
                        // Move to next element
                        self.state.cursor_down();
                        self.focus_current_in_browser().await;
                        break;
                    }
                    KeyCode::Up if is_combobox && !options.is_empty() => {
                        // Cycle to previous option
                        option_index = Some(match option_index {
                            None => options.len() - 1,
                            Some(0) => options.len() - 1,
                            Some(i) => i - 1,
                        });
                        input = options[option_index.unwrap()].clone();
                        cursor = input.len();
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Down if is_combobox && !options.is_empty() => {
                        // Cycle to next option
                        option_index = Some(match option_index {
                            None => 0,
                            Some(i) if i >= options.len() - 1 => 0,
                            Some(i) => i + 1,
                        });
                        input = options[option_index.unwrap()].clone();
                        cursor = input.len();
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Up if is_textarea => {
                        // Move cursor up one line
                        let before_cursor = &input[..cursor];
                        if let Some(current_line_start) = before_cursor.rfind('\n') {
                            // Find the column position in current line
                            let col = cursor - current_line_start - 1;
                            // Find start of previous line
                            let prev_content = &input[..current_line_start];
                            let prev_line_start = prev_content.rfind('\n').map(|i| i + 1).unwrap_or(0);
                            let prev_line_len = current_line_start - prev_line_start;
                            // Move to same column or end of previous line
                            cursor = prev_line_start + col.min(prev_line_len);
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    KeyCode::Down if is_textarea => {
                        // Move cursor down one line
                        let before_cursor = &input[..cursor];
                        let current_line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
                        let col = cursor - current_line_start;
                        // Find end of current line (next newline)
                        if let Some(rel_next_newline) = input[cursor..].find('\n') {
                            let next_line_start = cursor + rel_next_newline + 1;
                            // Find length of next line
                            let next_line_end = input[next_line_start..].find('\n')
                                .map(|i| next_line_start + i)
                                .unwrap_or(input.len());
                            let next_line_len = next_line_end - next_line_start;
                            // Move to same column or end of next line
                            cursor = next_line_start + col.min(next_line_len);
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    // Cursor movement
                    KeyCode::Left => {
                        if cursor > 0 {
                            // Move back one character (handle UTF-8)
                            cursor = input[..cursor].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    KeyCode::Right => {
                        if cursor < input.len() {
                            // Move forward one character (handle UTF-8)
                            cursor = input[cursor..].char_indices().nth(1).map(|(i, _)| cursor + i).unwrap_or(input.len());
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+A or Home: beginning of line
                        cursor = 0;
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+E or End: end of line
                        cursor = input.len();
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+K: delete from cursor to end of line
                        input.truncate(cursor);
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Esc => {
                        self.state.hide_input_prompt();
                        self.state.set_status("Cancelled");
                        break;
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+U: clear input (delete from start to cursor)
                        input = input[cursor..].to_string();
                        cursor = 0;
                        option_index = None;
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Ctrl+W: delete word before cursor
                        let before = &input[..cursor];
                        let trimmed = before.trim_end();
                        let word_start = trimmed.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
                        let after = &input[cursor..];
                        input = format!("{}{}", &input[..word_start], after);
                        cursor = word_start;
                        option_index = None;
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Char(c) => {
                        input.insert(cursor, c);
                        cursor += c.len_utf8();
                        option_index = None;
                        self.state.set_input_value_with_cursor(&input, cursor);
                        self.render()?;
                    }
                    KeyCode::Backspace => {
                        if cursor > 0 {
                            // Delete character before cursor
                            let prev_cursor = input[..cursor].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                            input.remove(prev_cursor);
                            cursor = prev_cursor;
                            option_index = None;
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    KeyCode::Delete => {
                        if cursor < input.len() {
                            // Delete character at cursor
                            input.remove(cursor);
                            option_index = None;
                            self.state.set_input_value_with_cursor(&input, cursor);
                            self.render()?;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Submit text to a field
    async fn submit_text_field(&mut self, input: &str, backend_id: i64, send_enter: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            // Focus the element
            let _ = client.call("DOM.focus", serde_json::json!({
                "backendNodeId": backend_id
            })).await;

            // Clear the field and set new value
            let _ = client.call("Runtime.evaluate", serde_json::json!({
                "expression": format!(
                    "if (document.activeElement) {{ document.activeElement.value = {}; document.activeElement.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
                    serde_json::to_string(input).unwrap_or_default()
                )
            })).await;

            // Send Enter key if requested (for search boxes etc)
            if send_enter {
                let _ = client.call("Input.dispatchKeyEvent", serde_json::json!({
                    "type": "keyDown",
                    "key": "Enter",
                    "code": "Enter",
                    "windowsVirtualKeyCode": 13,
                    "nativeVirtualKeyCode": 13
                })).await;
                let _ = client.call("Input.dispatchKeyEvent", serde_json::json!({
                    "type": "keyUp",
                    "key": "Enter",
                    "code": "Enter"
                })).await;
            }
        }
        Ok(())
    }

    async fn new_tab(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.state.add_tab();
        self.state.set_status("New tab opened");
        Ok(())
    }

    async fn close_current_tab(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.state.tabs.len() <= 1 {
            self.state.set_status("Cannot close last tab");
            return Ok(());
        }
        self.state.close_current_tab();
        self.state.set_status("Tab closed");
        Ok(())
    }

    fn switch_tab(&mut self, tab_num: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if tab_num < self.state.tabs.len() {
            self.state.current_tab_index = tab_num;
            self.state.set_status(&format!("Switched to tab {}", tab_num));
        }
        Ok(())
    }

    fn next_tab(&mut self) {
        let num_tabs = self.state.tabs.len();
        if num_tabs > 1 {
            self.state.current_tab_index = (self.state.current_tab_index + 1) % num_tabs;
            self.state.set_status(&format!("Tab {}/{}", self.state.current_tab_index + 1, num_tabs));
        }
    }

    fn prev_tab(&mut self) {
        let num_tabs = self.state.tabs.len();
        if num_tabs > 1 {
            self.state.current_tab_index = if self.state.current_tab_index == 0 {
                num_tabs - 1
            } else {
                self.state.current_tab_index - 1
            };
            self.state.set_status(&format!("Tab {}/{}", self.state.current_tab_index + 1, num_tabs));
        }
    }

    async fn open_file_dialog(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.state.show_input_prompt("File path: ");
        self.render()?;

        let mut input = String::new();
        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        self.state.hide_input_prompt();
                        if !input.is_empty() {
                            let file_url = format!("file://{}", input);
                            self.open_url(&file_url).await?;
                        }
                        break;
                    }
                    KeyCode::Esc => {
                        self.state.hide_input_prompt();
                        break;
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        self.state.set_input_value(&input);
                        self.render()?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn save_page(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get page HTML via CDP
        if let Some(ref client) = self.state.current_tab().page_client {
            let result = client.call("Runtime.evaluate", serde_json::json!({
                "expression": "document.documentElement.outerHTML"
            })).await?;

            if let Some(html) = result.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()) {
                self.state.show_input_prompt("Save to: ");
                self.render()?;

                let mut input = String::new();
                loop {
                    if let Event::Key(key) = event::read()? {
                        match key.code {
                            KeyCode::Enter => {
                                self.state.hide_input_prompt();
                                if !input.is_empty() {
                                    std::fs::write(&input, html)?;
                                    self.state.set_status(&format!("Saved to {}", input));
                                }
                                break;
                            }
                            KeyCode::Esc => {
                                self.state.hide_input_prompt();
                                break;
                            }
                            KeyCode::Char(c) => {
                                input.push(c);
                                self.state.set_input_value(&input);
                                self.render()?;
                            }
                            KeyCode::Backspace => {
                                input.pop();
                                self.state.set_input_value(&input);
                                self.render()?;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn show_downloads(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.state.set_status("Downloads: (none)");
        Ok(())
    }

    async fn print_page(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            // Use CDP to trigger print
            client.call("Page.printToPDF", serde_json::json!({
                "displayHeaderFooter": true,
                "printBackground": true
            })).await?;
            self.state.set_status("Print dialog opened (check browser)");
        }
        Ok(())
    }

    async fn refresh_page(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Save current element info to restore focus after refresh
        let saved_element = {
            let tab = self.state.current_tab();
            tab.current_node().map(|node| {
                (node.role_str().to_string(), node.name_str().to_string())
            })
        };

        self.state.set_status("Refreshing page...");
        self.render()?;

        if let Some(ref client) = self.state.current_tab().page_client {
            // Reload the page
            client.call("Page.reload", serde_json::json!({
                "ignoreCache": false
            })).await?;

            // Wait for page to load
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

            // Refresh accessibility tree
            self.refresh_current_tab().await?;

            // Try to restore focus to the same element
            if let Some((role, name)) = saved_element {
                if let Some(idx) = self.state.current_tab().find_by_role_and_name(&role, &name) {
                    self.state.current_tab_mut().cursor_index = idx;
                    self.state.set_status(&format!("Page refreshed (restored: {})", name));
                } else {
                    self.state.set_status("Page refreshed");
                }
            } else {
                self.state.set_status("Page refreshed");
            }
        }
        Ok(())
    }

    async fn toggle_viewport(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.state.toggle_viewport();
        let (width, height) = self.state.viewport_mode.dimensions();

        // Apply viewport to current page via CDP
        if let Some(ref client) = self.state.current_tab().page_client {
            client.call("Emulation.setDeviceMetricsOverride", serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": self.state.viewport_mode == VP::Mobile
            })).await?;

            // Refresh the page to apply layout changes
            self.refresh_page().await?;
        }

        // Save to config
        self.config.viewport_mode = self.state.viewport_mode;
        let _ = self.config.save();

        Ok(())
    }

    async fn activate_current_element(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get node info while we have immutable borrow
        let (role, name, backend_id) = {
            let tab = self.state.current_tab();
            if let Some(node) = tab.current_node() {
                (
                    node.role_str().to_lowercase(),
                    node.name_str().to_string(),
                    node.backend_dom_node_id,
                )
            } else {
                self.state.set_status("No element selected");
                return Ok(());
            }
        };

        self.state.set_status(&format!("Activating: {}", name));
        self.render()?;

        // Handle based on role
        match role.as_str() {
            "link" => {
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                } else {
                    self.state.set_status("Cannot activate: no DOM node");
                }
            }
            "button" => {
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                } else {
                    self.state.set_status("Cannot activate: no DOM node");
                }
            }
            "checkbox" | "radiobutton" => {
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                }
            }
            "textbox" | "textarea" | "textfield" | "searchbox" | "combobox" => {
                // Open text input prompt for the field
                if let Some(backend_id) = backend_id {
                    self.edit_text_field(&name, backend_id, &role).await?;
                } else {
                    self.state.set_status("Cannot edit: no DOM node");
                }
            }
            _ => {
                // Try to click any element
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                } else {
                    self.state.set_status(&format!("Cannot activate {} element", role));
                }
            }
        }

        Ok(())
    }

    async fn click_element(&mut self, backend_node_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Save current element and URL for potential focus restoration
        let (saved_element, saved_url) = {
            let tab = self.state.current_tab();
            let element = tab.current_node().map(|node| {
                (node.role_str().to_string(), node.name_str().to_string())
            });
            (element, tab.url.clone())
        };

        let client = match &self.state.current_tab().page_client {
            Some(c) => c,
            None => {
                self.state.set_status("No page loaded");
                return Ok(());
            }
        };

        // First, get the node's remote object
        let resolve_result = client.call("DOM.resolveNode", serde_json::json!({
            "backendNodeId": backend_node_id
        })).await?;

        if let Some(object_id) = resolve_result.get("object").and_then(|o| o.get("objectId")).and_then(|v| v.as_str()) {
            // Call click() on the element
            client.call("Runtime.callFunctionOn", serde_json::json!({
                "objectId": object_id,
                "functionDeclaration": "function() { this.click(); }",
                "returnByValue": true
            })).await?;

            // Short wait for any immediate DOM changes, then refresh
            self.state.set_status("Clicked...");
            self.render()?;
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

            // Refresh the accessibility tree
            self.refresh_current_tab().await?;

            // Check if URL changed (navigation vs AJAX update)
            let current_url = self.state.current_tab().url.clone();
            if current_url == saved_url {
                // Same page (AJAX update) - try to restore focus
                if let Some((role, name)) = saved_element {
                    if let Some(idx) = self.state.current_tab().find_by_role_and_name(&role, &name) {
                        self.state.current_tab_mut().cursor_index = idx;
                    }
                }
            }
            self.state.set_status("Ready");
        } else {
            // Fallback: try focus + evaluate click
            client.call("DOM.focus", serde_json::json!({
                "backendNodeId": backend_node_id
            })).await?;

            client.call("Runtime.evaluate", serde_json::json!({
                "expression": "document.activeElement.click()"
            })).await?;

            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            self.refresh_current_tab().await?;

            // Check if URL changed
            let current_url = self.state.current_tab().url.clone();
            if current_url == saved_url {
                if let Some((role, name)) = saved_element {
                    if let Some(idx) = self.state.current_tab().find_by_role_and_name(&role, &name) {
                        self.state.current_tab_mut().cursor_index = idx;
                    }
                }
            }
            self.state.set_status("Ready");
        }

        Ok(())
    }

    async fn toggle_current_element(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (role, backend_id) = {
            let tab = self.state.current_tab();
            if let Some(node) = tab.current_node() {
                (node.role_str().to_lowercase(), node.backend_dom_node_id)
            } else {
                return Ok(());
            }
        };

        match role.as_str() {
            "checkbox" | "radiobutton" => {
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                }
            }
            _ => {
                // Space on other elements also activates them
                if let Some(backend_id) = backend_id {
                    self.click_element(backend_id).await?;
                }
            }
        }
        Ok(())
    }

    /// Navigate using page's native tab order
    async fn tab_to_next_element(&mut self, shift: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = match &self.state.current_tab().page_client {
            Some(c) => c,
            None => {
                // Fallback to our own logic if no page loaded
                if shift {
                    self.state.prev_focusable();
                } else {
                    self.state.next_focusable();
                }
                return Ok(());
            }
        };

        // Dispatch Tab or Shift+Tab key to the page
        let modifiers = if shift { 8 } else { 0 }; // 8 = Shift modifier

        client.call("Input.dispatchKeyEvent", serde_json::json!({
            "type": "keyDown",
            "key": "Tab",
            "code": "Tab",
            "windowsVirtualKeyCode": 9,
            "nativeVirtualKeyCode": 9,
            "modifiers": modifiers
        })).await?;

        client.call("Input.dispatchKeyEvent", serde_json::json!({
            "type": "keyUp",
            "key": "Tab",
            "code": "Tab",
            "windowsVirtualKeyCode": 9,
            "nativeVirtualKeyCode": 9,
            "modifiers": modifiers
        })).await?;

        // Give the page a moment to update focus
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Query which element is now focused
        let result = client.call("Runtime.evaluate", serde_json::json!({
            "expression": r#"
                (function() {
                    const el = document.activeElement;
                    if (!el || el === document.body) return null;
                    return {
                        tagName: el.tagName,
                        id: el.id,
                        className: el.className,
                        textContent: (el.textContent || '').substring(0, 100).trim(),
                        ariaLabel: el.getAttribute('aria-label') || '',
                        name: el.getAttribute('name') || ''
                    };
                })()
            "#,
            "returnByValue": true
        })).await?;

        // Try to find this element in our accessibility tree
        if let Some(value) = result.get("result").and_then(|r| r.get("value")) {
            if !value.is_null() {
                let text = value.get("textContent").and_then(|v| v.as_str()).unwrap_or("");
                let aria_label = value.get("ariaLabel").and_then(|v| v.as_str()).unwrap_or("");
                let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("");

                // Try to find by aria-label first, then by text content, then by name
                let search_terms = [aria_label, text, name];
                for term in search_terms {
                    if !term.is_empty() {
                        if let Some(idx) = self.state.current_tab().find_by_name(term) {
                            self.state.current_tab_mut().cursor_index = idx;
                            self.state.set_status(&format!("Focused: {}", term));
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Fallback: use our own navigation
        if shift {
            self.state.prev_focusable();
        } else {
            self.state.next_focusable();
        }
        Ok(())
    }

    async fn send_key_to_element(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            match key.code {
                KeyCode::Char(c) => {
                    // Handle character input - account for shift
                    let text = if key.modifiers.contains(KeyModifiers::SHIFT) {
                        c.to_uppercase().to_string()
                    } else {
                        c.to_string()
                    };
                    client.call("Input.insertText", serde_json::json!({
                        "text": text
                    })).await?;
                }
                KeyCode::Enter => {
                    // Dispatch Enter key event
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyDown",
                        "key": "Enter",
                        "code": "Enter",
                        "windowsVirtualKeyCode": 13,
                        "nativeVirtualKeyCode": 13
                    })).await?;
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyUp",
                        "key": "Enter",
                        "code": "Enter",
                        "windowsVirtualKeyCode": 13,
                        "nativeVirtualKeyCode": 13
                    })).await?;
                }
                KeyCode::Backspace => {
                    // Dispatch Backspace key event
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyDown",
                        "key": "Backspace",
                        "code": "Backspace",
                        "windowsVirtualKeyCode": 8,
                        "nativeVirtualKeyCode": 8
                    })).await?;
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyUp",
                        "key": "Backspace",
                        "code": "Backspace",
                        "windowsVirtualKeyCode": 8,
                        "nativeVirtualKeyCode": 8
                    })).await?;
                }
                KeyCode::Tab => {
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyDown",
                        "key": "Tab",
                        "code": "Tab",
                        "windowsVirtualKeyCode": 9,
                        "nativeVirtualKeyCode": 9
                    })).await?;
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyUp",
                        "key": "Tab",
                        "code": "Tab",
                        "windowsVirtualKeyCode": 9,
                        "nativeVirtualKeyCode": 9
                    })).await?;
                }
                KeyCode::Left => {
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyDown",
                        "key": "ArrowLeft",
                        "code": "ArrowLeft",
                        "windowsVirtualKeyCode": 37,
                        "nativeVirtualKeyCode": 37
                    })).await?;
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyUp",
                        "key": "ArrowLeft",
                        "code": "ArrowLeft"
                    })).await?;
                }
                KeyCode::Right => {
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyDown",
                        "key": "ArrowRight",
                        "code": "ArrowRight",
                        "windowsVirtualKeyCode": 39,
                        "nativeVirtualKeyCode": 39
                    })).await?;
                    client.call("Input.dispatchKeyEvent", serde_json::json!({
                        "type": "keyUp",
                        "key": "ArrowRight",
                        "code": "ArrowRight"
                    })).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn refresh_current_tab(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let tab = self.state.current_tab_mut();
        if let Some(ref client) = tab.page_client {
            let accessibility = AccessibilityDomain::new(client);
            let tree = accessibility.get_full_tree().await?;
            tab.set_tree(tree);
        }
        Ok(())
    }

    // Media control methods
    async fn try_media_play_pause(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            let status = MediaController::get_status(client).await?;
            if status.has_video {
                let playing = MediaController::toggle_play(client).await?;
                self.state.set_status(if playing { "▶ Playing" } else { "⏸ Paused" });
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn media_toggle_mute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            let muted = MediaController::toggle_mute(client).await?;
            self.state.set_status(if muted { "🔇 Muted" } else { "🔊 Unmuted" });
        }
        Ok(())
    }

    async fn media_seek(&mut self, seconds: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            MediaController::seek_relative(client, seconds).await?;
            let status = MediaController::get_status(client).await?;
            self.state.set_status(&format!(
                "⏱ {} / {}",
                MediaStatus::format_time(status.current_time),
                MediaStatus::format_time(status.duration)
            ));
        }
        Ok(())
    }

    async fn media_seek_percent(&mut self, percent: u8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            MediaController::seek_percent(client, percent).await?;
            self.state.set_status(&format!("⏱ Jumped to {}%", percent));
        }
        Ok(())
    }

    async fn media_adjust_volume(&mut self, delta: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            let volume = MediaController::adjust_volume(client, delta).await?;
            self.state.set_status(&format!("🔊 Volume: {}%", (volume * 100.0) as u8));
        }
        Ok(())
    }

    async fn media_adjust_speed(&mut self, delta: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            // Get current speed and adjust
            let result = client.call("Runtime.evaluate", serde_json::json!({
                "expression": r#"
                    (function() {
                        const v = document.querySelector('video');
                        if (!v) return 1;
                        return v.playbackRate;
                    })()
                "#,
                "returnByValue": true
            })).await?;

            let current = result.get("result")
                .and_then(|r| r.get("value"))
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);

            let new_speed = (current + delta).max(0.25).min(3.0);
            MediaController::set_speed(client, new_speed).await?;
            self.state.set_status(&format!("⏩ Speed: {:.2}x", new_speed));
        }
        Ok(())
    }

    async fn media_toggle_captions(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref client) = self.state.current_tab().page_client {
            let enabled = MediaController::toggle_captions(client).await?;
            self.state.set_status(if enabled { "CC: On" } else { "CC: Off" });
        }
        Ok(())
    }

    fn render(&self) -> io::Result<()> {
        render::render(&self.state, &self.config)
    }
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                ' ' => result.push('+'),
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
