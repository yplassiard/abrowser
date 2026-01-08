//! Browser state management

use std::sync::Arc;
use crate::accessibility::{AXNode, AXTree};
use crate::backend::PageSession;
// TODO: Remove CdpClient import when Phase 4 migration is complete
use crate::cdp::CdpClient;
use super::config::ViewportMode;
use super::media::MediaStatus;

/// Focus mode determines how keys are handled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Keys navigate the page
    Navigation,
    /// Keys are passed to focused element
    Focus,
}

/// History entry for a visited page
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
}

/// A browser tab
pub struct Tab {
    pub url: String,
    pub title: String,
    /// Page session (backend-agnostic interface) - Arc for sharing with background tasks
    pub session: Option<Arc<dyn PageSession>>,
    /// Legacy CDP client (TODO: remove in Phase 6)
    pub page_client: Option<CdpClient>,
    tree: Option<AXTree>,
    pub nodes: Vec<NodeRef>,
    pub cursor_index: usize,
    pub scroll_offset: usize,
    pub visited_links: Vec<String>,
    /// Loading progress (0-100), None if not loading
    pub loading_progress: Option<u8>,
    /// Number of pending network requests
    pub pending_requests: usize,
    /// Total requests started for this page load
    pub total_requests: usize,
    /// Whether the tree needs refreshing due to DOM changes
    pub needs_tree_refresh: bool,
    /// Navigation history for this tab
    pub history: Vec<HistoryEntry>,
    /// Current position in history (index into history vec)
    pub history_index: usize,
    /// When loading started (for timeout fallback)
    pub loading_started: Option<std::time::Instant>,
    /// When last DOM change occurred (for debounced refresh)
    pub last_dom_change: Option<std::time::Instant>,
}

/// Reference to a node in the tree
#[derive(Clone)]
#[allow(dead_code)]
pub struct NodeRef {
    pub node_id: String,
    pub role: String,
    pub name: String,
    pub has_handle: bool,
}

impl Tab {
    pub fn new() -> Self {
        Self {
            url: String::new(),
            title: "New Tab".to_string(),
            session: None,
            page_client: None,
            tree: None,
            nodes: Vec::new(),
            cursor_index: 0,
            scroll_offset: 0,
            visited_links: Vec::new(),
            loading_progress: None,
            pending_requests: 0,
            total_requests: 0,
            needs_tree_refresh: false,
            history: Vec::new(),
            history_index: 0,
            loading_started: None,
            last_dom_change: None,
        }
    }

    /// Add current page to history (called after navigation completes)
    pub fn push_history(&mut self) {
        if self.url.is_empty() {
            return;
        }

        // If we navigated back and then to a new page, truncate forward history
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }

        // Don't add duplicate consecutive entries
        if let Some(last) = self.history.last() {
            if last.url == self.url {
                // Just update title if changed
                if last.title != self.title {
                    self.history.last_mut().unwrap().title = self.title.clone();
                }
                return;
            }
        }

        self.history.push(HistoryEntry {
            url: self.url.clone(),
            title: self.title.clone(),
        });
        self.history_index = self.history.len().saturating_sub(1);
    }

    /// Check if we can go back in history
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    /// Check if we can go forward in history
    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// Go back in history, returns the URL to navigate to
    pub fn go_back(&mut self) -> Option<String> {
        if self.can_go_back() {
            self.history_index -= 1;
            Some(self.history[self.history_index].url.clone())
        } else {
            None
        }
    }

    /// Go forward in history, returns the URL to navigate to
    pub fn go_forward(&mut self) -> Option<String> {
        if self.can_go_forward() {
            self.history_index += 1;
            Some(self.history[self.history_index].url.clone())
        } else {
            None
        }
    }

    /// Start loading a new page
    pub fn start_loading(&mut self) {
        self.loading_progress = Some(0);
        self.pending_requests = 0;
        self.total_requests = 0;
        self.needs_tree_refresh = false;
        self.loading_started = Some(std::time::Instant::now());
    }

    /// Update loading progress based on network requests
    pub fn update_loading_progress(&mut self) {
        if self.total_requests > 0 {
            let completed = self.total_requests.saturating_sub(self.pending_requests);
            let progress = (completed * 100 / self.total_requests).min(99) as u8;
            self.loading_progress = Some(progress);
        }
    }

    /// Mark loading as complete
    pub fn finish_loading(&mut self) {
        self.loading_progress = None;
        self.pending_requests = 0;
        self.loading_started = None;
    }

    pub fn set_tree(&mut self, tree: AXTree) {
        // Save current node info for cursor restoration
        let prev_node_info = self.nodes.get(self.cursor_index).map(|n| {
            (n.role.clone(), n.name.clone(), n.node_id.clone())
        });

        // Build node references from linearized tree
        let linearized = tree.linearize();
        self.nodes = linearized
            .iter()
            .map(|n| NodeRef {
                node_id: n.id.clone(),
                role: format!("{:?}", n.role),
                name: n.name.clone(),
                has_handle: n.handle.is_some(),
            })
            .collect();

        // Find focused node from CDP and restore cursor
        let mut new_cursor = None;

        // First priority: find the focused node from the accessibility tree
        for (i, node) in linearized.iter().enumerate() {
            if node.state.focused {
                new_cursor = Some(i);
                break;
            }
        }

        // Second priority: try to find the same node by ID
        if new_cursor.is_none() {
            if let Some((_, _, ref prev_id)) = prev_node_info {
                for (i, node_ref) in self.nodes.iter().enumerate() {
                    if &node_ref.node_id == prev_id {
                        new_cursor = Some(i);
                        break;
                    }
                }
            }
        }

        // Third priority: try to find by role and name
        if new_cursor.is_none() {
            if let Some((ref prev_role, ref prev_name, _)) = prev_node_info {
                new_cursor = self.find_by_role_and_name(prev_role, prev_name);
            }
        }

        self.tree = Some(tree);
        self.cursor_index = new_cursor.unwrap_or(0);
    }

    pub fn current_node(&self) -> Option<&AXNode> {
        let node_ref = self.nodes.get(self.cursor_index)?;
        self.tree.as_ref()?.get(&node_ref.node_id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get_node(&self, index: usize) -> Option<&AXNode> {
        let node_ref = self.nodes.get(index)?;
        self.tree.as_ref()?.get(&node_ref.node_id)
    }

    pub fn nodes_iter(&self) -> impl Iterator<Item = &AXNode> {
        self.nodes.iter().filter_map(|nr| {
            self.tree.as_ref().and_then(|t| t.get(&nr.node_id))
        })
    }

    /// Find next element of given role
    pub fn find_next(&self, role: &str) -> Option<usize> {
        let roles = Self::expand_role(role);
        for i in (self.cursor_index + 1)..self.nodes.len() {
            if roles.iter().any(|r| self.nodes[i].role.eq_ignore_ascii_case(r)) {
                return Some(i);
            }
        }
        // Wrap around
        for i in 0..self.cursor_index {
            if roles.iter().any(|r| self.nodes[i].role.eq_ignore_ascii_case(r)) {
                return Some(i);
            }
        }
        None
    }

    /// Find previous element of given role
    pub fn find_prev(&self, role: &str) -> Option<usize> {
        let roles = Self::expand_role(role);
        for i in (0..self.cursor_index).rev() {
            if roles.iter().any(|r| self.nodes[i].role.eq_ignore_ascii_case(r)) {
                return Some(i);
            }
        }
        // Wrap around
        for i in (self.cursor_index + 1..self.nodes.len()).rev() {
            if roles.iter().any(|r| self.nodes[i].role.eq_ignore_ascii_case(r)) {
                return Some(i);
            }
        }
        None
    }

    /// Expand role name to all matching CDP roles
    fn expand_role(role: &str) -> Vec<&'static str> {
        match role {
            "heading" => vec!["heading", "Heading"],
            "link" => vec!["link", "Link"],
            "button" => vec!["button", "Button"],
            "checkbox" => vec!["checkbox", "CheckBox"],
            "radiobutton" => vec!["radiobutton", "RadioButton", "radio"],
            "list" => vec!["list", "List", "listbox", "ListBox"],
            "table" => vec!["table", "Table"],
            "textbox" => vec![
                "textbox", "TextField", "textarea", "TextArea",
                "searchbox", "SearchBox", "spinbutton", "SpinButton",
                "editabletext", "EditableText",
            ],
            "combobox" => vec!["combobox", "ComboBox", "listbox", "ListBox"],
            "image" => vec!["image", "Image", "img", "Img", "graphic", "Graphic"],
            _ => vec![],
        }
    }

    /// Find element by role and name (for restoring focus after refresh)
    pub fn find_by_role_and_name(&self, role: &str, name: &str) -> Option<usize> {
        // First try exact match
        for (i, node) in self.nodes.iter().enumerate() {
            if node.role.eq_ignore_ascii_case(role) && node.name == name {
                return Some(i);
            }
        }
        // If no exact match, try matching just by name (role might have changed)
        if !name.is_empty() {
            for (i, node) in self.nodes.iter().enumerate() {
                if node.name == name {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Find element by name (for tab navigation)
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        // Exact match first
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name == name {
                return Some(i);
            }
        }
        // Partial match (name contains search term)
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name.contains(name) || name.contains(&node.name) {
                return Some(i);
            }
        }
        None
    }

    /// Find next element matching text pattern (case-insensitive)
    pub fn find_next_text_match(&self, pattern: &str) -> Option<usize> {
        if pattern.is_empty() {
            return None;
        }
        let pattern_lower = pattern.to_lowercase();
        // Search from current position forward
        for i in (self.cursor_index + 1)..self.nodes.len() {
            if self.nodes[i].name.to_lowercase().contains(&pattern_lower) {
                return Some(i);
            }
        }
        // Wrap around
        for i in 0..=self.cursor_index {
            if self.nodes[i].name.to_lowercase().contains(&pattern_lower) {
                return Some(i);
            }
        }
        None
    }

    /// Find previous element matching text pattern (case-insensitive)
    pub fn find_prev_text_match(&self, pattern: &str) -> Option<usize> {
        if pattern.is_empty() {
            return None;
        }
        let pattern_lower = pattern.to_lowercase();
        // Search from current position backward
        for i in (0..self.cursor_index).rev() {
            if self.nodes[i].name.to_lowercase().contains(&pattern_lower) {
                return Some(i);
            }
        }
        // Wrap around
        for i in (self.cursor_index..self.nodes.len()).rev() {
            if self.nodes[i].name.to_lowercase().contains(&pattern_lower) {
                return Some(i);
            }
        }
        None
    }

    /// Find next visited link
    pub fn find_next_visited(&self) -> Option<usize> {
        // TODO: Track visited links properly
        self.find_next("link")
    }

    /// Find previous visited link
    pub fn find_prev_visited(&self) -> Option<usize> {
        self.find_prev("link")
    }

    /// Check if a role is a landmark
    fn is_landmark_role(role: &str) -> bool {
        matches!(
            role.to_lowercase().as_str(),
            "main" | "navigation" | "banner" | "contentinfo" |
            "complementary" | "search" | "region" | "form"
        )
    }

    /// Find next landmark
    pub fn find_next_landmark(&self) -> Option<usize> {
        for i in (self.cursor_index + 1)..self.nodes.len() {
            if Self::is_landmark_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        // Wrap around
        for i in 0..self.cursor_index {
            if Self::is_landmark_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        None
    }

    /// Find previous landmark
    pub fn find_prev_landmark(&self) -> Option<usize> {
        for i in (0..self.cursor_index).rev() {
            if Self::is_landmark_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        // Wrap around
        for i in (self.cursor_index + 1..self.nodes.len()).rev() {
            if Self::is_landmark_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        None
    }

    /// Check if a role is focusable
    fn is_focusable_role(role: &str) -> bool {
        matches!(
            role.to_lowercase().as_str(),
            "link" | "button" | "textbox" | "textfield" | "textarea" |
            "checkbox" | "radiobutton" | "radio" | "combobox" | "listbox" |
            "menuitem" | "option" | "tab" | "switch" | "slider" | "spinbutton"
        )
    }

    /// Find next focusable element (Tab)
    pub fn find_next_focusable(&self) -> Option<usize> {
        for i in (self.cursor_index + 1)..self.nodes.len() {
            if Self::is_focusable_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        // Wrap around
        for i in 0..self.cursor_index {
            if Self::is_focusable_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        None
    }

    /// Find previous focusable element (Shift+Tab)
    pub fn find_prev_focusable(&self) -> Option<usize> {
        for i in (0..self.cursor_index).rev() {
            if Self::is_focusable_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        // Wrap around
        for i in (self.cursor_index + 1..self.nodes.len()).rev() {
            if Self::is_focusable_role(&self.nodes[i].role) {
                return Some(i);
            }
        }
        None
    }
}

impl Default for Tab {
    fn default() -> Self {
        Self::new()
    }
}

/// Main browser state
pub struct BrowserState {
    pub tabs: Vec<Tab>,
    pub current_tab_index: usize,
    pub focus_mode: FocusMode,
    pub viewport_mode: ViewportMode,
    pub status_message: String,
    pub input_prompt: Option<String>,
    pub input_value: String,
    pub input_cursor: usize,
    pub terminal_height: u16,
    pub media_status: Option<MediaStatus>,
    pub search_pattern: String,
    pub search_forward: bool,
    /// Whether the screen needs to be redrawn
    pub needs_render: bool,
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            tabs: vec![Tab::new()],
            current_tab_index: 0,
            focus_mode: FocusMode::Navigation,
            viewport_mode: ViewportMode::default(),
            status_message: String::new(),
            input_prompt: None,
            input_value: String::new(),
            input_cursor: 0,
            terminal_height: 24,
            media_status: None,
            search_pattern: String::new(),
            search_forward: true,
            needs_render: true, // Initial render needed
        }
    }

    /// Mark that the screen needs to be redrawn
    pub fn mark_dirty(&mut self) {
        self.needs_render = true;
    }

    /// Clear the dirty flag after rendering
    pub fn clear_dirty(&mut self) {
        self.needs_render = false;
    }

    pub fn with_viewport(mut self, mode: ViewportMode) -> Self {
        self.viewport_mode = mode;
        self
    }

    pub fn toggle_viewport(&mut self) {
        self.viewport_mode = self.viewport_mode.toggle();
        self.status_message = format!("Viewport: {}", self.viewport_mode.as_str());
    }

    pub fn current_tab(&self) -> &Tab {
        &self.tabs[self.current_tab_index]
    }

    pub fn current_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.current_tab_index]
    }

    pub fn add_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.current_tab_index = self.tabs.len() - 1;
    }

    pub fn close_current_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.tabs.remove(self.current_tab_index);
            if self.current_tab_index >= self.tabs.len() {
                self.current_tab_index = self.tabs.len() - 1;
            }
        }
    }

    pub fn toggle_focus_mode(&mut self) {
        self.focus_mode = match self.focus_mode {
            FocusMode::Navigation => FocusMode::Focus,
            FocusMode::Focus => FocusMode::Navigation,
        };
        self.status_message = format!("Mode: {:?}", self.focus_mode);
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = msg.to_string();
    }

    pub fn show_input_prompt(&mut self, prompt: &str) {
        self.input_prompt = Some(prompt.to_string());
        self.input_value.clear();
        self.input_cursor = 0;
    }

    pub fn hide_input_prompt(&mut self) {
        self.input_prompt = None;
        self.input_value.clear();
        self.input_cursor = 0;
    }

    pub fn set_input_value(&mut self, value: &str) {
        self.input_value = value.to_string();
        self.input_cursor = value.len();
    }

    pub fn set_input_value_with_cursor(&mut self, value: &str, cursor: usize) {
        self.input_value = value.to_string();
        self.input_cursor = cursor.min(value.len());
    }

    // Cursor navigation
    pub fn cursor_up(&mut self) {
        let tab = self.current_tab_mut();
        if tab.cursor_index > 0 {
            tab.cursor_index -= 1;
            self.ensure_cursor_visible();
        }
    }

    pub fn cursor_down(&mut self) {
        let tab = self.current_tab_mut();
        if tab.cursor_index + 1 < tab.node_count() {
            tab.cursor_index += 1;
            self.ensure_cursor_visible();
        }
    }

    pub fn cursor_to_top(&mut self) {
        let tab = self.current_tab_mut();
        tab.cursor_index = 0;
        tab.scroll_offset = 0;
    }

    pub fn cursor_to_bottom(&mut self) {
        let tab = self.current_tab_mut();
        let count = tab.node_count();
        if count > 0 {
            tab.cursor_index = count - 1;
            self.ensure_cursor_visible();
        }
    }

    pub fn cursor_to_line_start(&mut self) {
        // In a terminal context, this is same as cursor_up to previous section
        // For now, same as top
        self.cursor_to_top();
    }

    pub fn cursor_to_line_end(&mut self) {
        self.cursor_to_bottom();
    }

    pub fn page_up(&mut self) {
        let page_size = (self.terminal_height as usize).saturating_sub(4);
        let tab = self.current_tab_mut();
        tab.cursor_index = tab.cursor_index.saturating_sub(page_size);
        tab.scroll_offset = tab.scroll_offset.saturating_sub(page_size);
    }

    pub fn page_down(&mut self) {
        let page_size = (self.terminal_height as usize).saturating_sub(4);
        let tab = self.current_tab_mut();
        let max = tab.node_count().saturating_sub(1);
        tab.cursor_index = (tab.cursor_index + page_size).min(max);
        self.ensure_cursor_visible();
    }

    fn ensure_cursor_visible(&mut self) {
        let visible_lines = (self.terminal_height as usize).saturating_sub(4);
        let tab = self.current_tab_mut();

        if tab.cursor_index < tab.scroll_offset {
            tab.scroll_offset = tab.cursor_index;
        } else if tab.cursor_index >= tab.scroll_offset + visible_lines {
            tab.scroll_offset = tab.cursor_index - visible_lines + 1;
        }
    }

    // Element navigation
    pub fn next_element(&mut self, role: &str) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_next(role) {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn prev_element(&mut self, role: &str) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_prev(role) {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn next_visited_link(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_next_visited() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn prev_visited_link(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_prev_visited() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn next_landmark(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_next_landmark() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn prev_landmark(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_prev_landmark() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn next_focusable(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_next_focusable() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    pub fn prev_focusable(&mut self) {
        let tab = self.current_tab_mut();
        if let Some(idx) = tab.find_prev_focusable() {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
        }
    }

    /// Search for next match of current pattern
    pub fn search_next(&mut self) -> bool {
        if self.search_pattern.is_empty() {
            return false;
        }
        let pattern = self.search_pattern.clone();
        let forward = self.search_forward;
        let tab = self.current_tab_mut();
        let found = if forward {
            tab.find_next_text_match(&pattern)
        } else {
            tab.find_prev_text_match(&pattern)
        };
        if let Some(idx) = found {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    /// Search for previous match (opposite direction)
    pub fn search_prev(&mut self) -> bool {
        if self.search_pattern.is_empty() {
            return false;
        }
        let pattern = self.search_pattern.clone();
        let forward = self.search_forward;
        let tab = self.current_tab_mut();
        let found = if forward {
            tab.find_prev_text_match(&pattern)
        } else {
            tab.find_next_text_match(&pattern)
        };
        if let Some(idx) = found {
            tab.cursor_index = idx;
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }
}

impl Default for BrowserState {
    fn default() -> Self {
        Self::new()
    }
}
