//! Firefox page session implementing the PageSession trait

use crate::accessibility::{AXNode, AXTree, Role};
use crate::backend::{
    BackendError, BackendKind, BackendResult, BrowserEvent, Key, KeyModifiers, MediaStatus,
    NodeHandle, NodeHandleInner, PageSession,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

use super::actors::{self, ActorInfo};
use super::client::RdpClient;

/// Firefox page session
pub struct FirefoxSession {
    client: Arc<RdpClient>,
    actors: Mutex<ActorInfo>,
    #[allow(dead_code)]
    viewport: Option<(u32, u32)>,
}

impl FirefoxSession {
    /// Create a new Firefox session
    pub async fn new(
        client: Arc<RdpClient>,
        url: &str,
        viewport: Option<(u32, u32)>,
    ) -> BackendResult<Self> {
        // Discover initial actors
        let actors = ActorInfo::discover(&client)
            .map_err(|e| BackendError::Protocol(format!("Failed to discover actors: {}", e)))?;

        let session = Self {
            client,
            actors: Mutex::new(actors),
            viewport,
        };

        // Navigate to the URL if not about:blank
        if url != "about:blank" {
            session.navigate_internal(url).await?;
        }

        Ok(session)
    }

    /// Internal navigation that handles actor refresh
    async fn navigate_internal(&self, url: &str) -> BackendResult<()> {
        let tab_descriptor = {
            let actors = self.actors.lock().unwrap();
            actors.tab_descriptor.clone()
        };

        // Navigate using the target
        {
            let target = {
                let actors = self.actors.lock().unwrap();
                actors.target.clone()
            };
            let _ = actors::navigate(&self.client, &target, url);
        }

        // Wait for navigation to complete
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // After cross-origin navigation, actors change. Re-discover them.
        self.refresh_actors(&tab_descriptor)?;

        Ok(())
    }

    /// Refresh actors from the tab descriptor (needed after cross-origin navigation)
    fn refresh_actors(&self, tab_descriptor: &str) -> BackendResult<()> {
        // Get fresh target actors from the tab descriptor
        let target_actors = ActorInfo::get_target(&self.client, tab_descriptor)
            .map_err(|e| BackendError::Protocol(format!("Failed to get target: {}", e)))?;

        // Attach to new target
        let _ = ActorInfo::attach_target(&self.client, &target_actors.target);

        // Get walker if we have accessibility actor
        let accessibility_walker = target_actors.accessibility.as_ref().and_then(|a| {
            ActorInfo::get_walker(&self.client, a).ok()
        });

        // Update the actors
        {
            let mut actors = self.actors.lock().unwrap();
            actors.target = target_actors.target;
            actors.console = target_actors.console;
            actors.accessibility = target_actors.accessibility;
            actors.accessibility_walker = accessibility_walker;
        }

        Ok(())
    }

    /// Get the console actor, discovering if needed
    fn get_console(&self) -> BackendResult<String> {
        let actors = self.actors.lock().unwrap();
        actors.console.clone().ok_or_else(|| {
            BackendError::Protocol("Console actor not available".to_string())
        })
    }

    /// Evaluate JavaScript using the console actor
    fn eval_js_sync(&self, script: &str) -> BackendResult<Value> {
        let console = self.get_console()?;

        // Send the evaluation request
        let response = self.client.send(&console, "evaluateJSAsync", json!({
            "text": script,
            "eager": false
        }))
        .map_err(|e| BackendError::Protocol(e.to_string()))?;

        // Check if we got the result directly
        if let Some(result) = response.get("result") {
            if let Some(value) = result.get("value") {
                return Ok(value.clone());
            }
            if result.get("type").and_then(|t| t.as_str()) == Some("undefined") {
                return Ok(Value::Null);
            }
        }

        // For async evaluation, we get a resultID and need to wait for evaluationResult event
        if let Some(result_id) = response.get("resultID").and_then(|v| v.as_str()) {
            // Wait for the result event
            for _ in 0..50 {
                // Check for events
                if let Some(event) = self.client.try_recv_event() {
                    if event.event_type == "evaluationResult" {
                        if let Some(event_result_id) = event.data.get("resultID").and_then(|v| v.as_str()) {
                            if event_result_id == result_id {
                                // Found our result
                                if let Some(result) = event.data.get("result") {
                                    if let Some(value) = result.get("value") {
                                        return Ok(value.clone());
                                    }
                                    // Handle object results
                                    if let Some(preview) = result.get("preview").and_then(|p| p.get("ownProperties")) {
                                        return Ok(preview.clone());
                                    }
                                    return Ok(result.clone());
                                }
                                return Ok(event.data);
                            }
                        }
                    }
                }

                // Poll for more messages
                let _ = self.client.poll();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            return Err(BackendError::Protocol("Timeout waiting for JS evaluation result".into()));
        }

        Ok(response)
    }
}

// Note: RdpClient is not Send+Sync safe in its current form
// We need to wrap operations carefully
unsafe impl Send for FirefoxSession {}
unsafe impl Sync for FirefoxSession {}

#[async_trait]
impl PageSession for FirefoxSession {
    fn kind(&self) -> BackendKind {
        BackendKind::Firefox
    }

    // ========== Navigation ==========

    async fn navigate(&self, url: &str) -> BackendResult<()> {
        self.navigate_internal(url).await
    }

    async fn reload(&self) -> BackendResult<()> {
        let target = {
            let actors = self.actors.lock().unwrap();
            actors.target.clone()
        };

        actors::reload(&self.client, &target)
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(())
    }

    async fn current_url(&self) -> BackendResult<String> {
        let tab_descriptor = {
            let actors = self.actors.lock().unwrap();
            actors.tab_descriptor.clone()
        };

        actors::get_url(&self.client, &tab_descriptor)
            .map_err(|e| BackendError::Protocol(e.to_string()))
    }

    async fn title(&self) -> BackendResult<String> {
        let tab_descriptor = {
            let actors = self.actors.lock().unwrap();
            actors.tab_descriptor.clone()
        };

        actors::get_title(&self.client, &tab_descriptor)
            .map_err(|e| BackendError::Protocol(e.to_string()))
    }

    // ========== Accessibility ==========

    async fn enable_accessibility(&self) -> BackendResult<()> {
        let parent_accessibility = {
            let actors = self.actors.lock().unwrap();
            actors.parent_accessibility.clone()
        };

        if let Some(pa) = parent_accessibility {
            ActorInfo::enable_accessibility(&self.client, &pa)
                .map_err(|e| BackendError::Protocol(e.to_string()))?;
        }

        Ok(())
    }

    async fn get_accessibility_tree(&self) -> BackendResult<AXTree> {

        // Use JavaScript to build accessibility tree since native accessibility
        // doesn't work in headless mode without an AT connected
        // Return as JSON string to avoid RDP preview/grip issues
        let script = r#"
            (function() {
                let nodeId = 0;

                function getRole(el) {
                    if (!el || !el.tagName) return null;
                    const role = el.getAttribute && el.getAttribute('role');
                    if (role) return role;
                    const tag = el.tagName.toLowerCase();
                    const roles = {
                        'a': el.href ? 'link' : null,
                        'button': 'button',
                        'input': {'checkbox':'checkbox','radio':'radio','submit':'button','text':'textbox','password':'textbox','search':'searchbox','email':'textbox','url':'textbox','tel':'textbox','number':'spinbutton'}[el.type] || 'textbox',
                        'textarea': 'textbox',
                        'select': 'combobox',
                        'h1':'heading','h2':'heading','h3':'heading','h4':'heading','h5':'heading','h6':'heading',
                        'p':'paragraph',
                        'ul':'list','ol':'list',
                        'li':'listitem',
                        'img':'img',
                        'nav':'navigation',
                        'main':'main',
                        'header':'banner',
                        'footer':'contentinfo',
                        'article':'article',
                        'section':'region',
                        'aside':'complementary',
                        'form':'form',
                        'table':'table',
                        'tr':'row',
                        'th':'columnheader',
                        'td':'cell'
                    };
                    return roles[tag] || null;
                }

                function getName(el) {
                    if (!el) return '';
                    const label = el.getAttribute && (el.getAttribute('aria-label') || el.getAttribute('alt') || el.getAttribute('title'));
                    if (label) return label;
                    if (el.tagName === 'IMG') return el.alt || '';
                    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') return el.value || el.placeholder || '';
                    if (el.children && el.children.length === 0 && el.textContent) {
                        return el.textContent.trim().substring(0, 200);
                    }
                    return '';
                }

                function getLevel(el) {
                    const tag = el.tagName;
                    if (tag && tag.match(/^H[1-6]$/i)) {
                        return parseInt(tag[1]);
                    }
                    return 0;
                }

                function walk(el, parentId) {
                    if (!el || el.nodeType !== 1) return null;
                    const style = window.getComputedStyle(el);
                    if (style.display === 'none' || style.visibility === 'hidden') return null;

                    const id = 'n' + (++nodeId);
                    const role = getRole(el);
                    const name = getName(el);
                    const children = [];

                    for (const child of el.children) {
                        const childNode = walk(child, id);
                        if (childNode) children.push(childNode);
                    }

                    // Include if has role, name, or interesting children
                    if (role || name || children.length > 0) {
                        return {
                            id: id,
                            role: role || 'generic',
                            name: name,
                            level: getLevel(el),
                            parentId: parentId,
                            childIds: children.map(c => c.id),
                            children: children,
                            focusable: el.tabIndex >= 0,
                            url: el.href || null
                        };
                    }
                    return null;
                }

                const body = document.body || document.documentElement;
                const root = walk(body, null);
                if (root) {
                    root.role = 'document';
                    root.name = document.title || '';
                }
                // Return as JSON string to avoid RDP preview issues
                return JSON.stringify(root);
            })()
        "#;

        let result = self.eval_js_sync(script)?;

        // Parse the JSON string result
        let tree_json = if let Some(json_str) = result.as_str() {
            serde_json::from_str::<Value>(json_str)
                .map_err(|e| BackendError::Protocol(format!("Failed to parse accessibility JSON: {}", e)))?
        } else {
            // Fallback to direct object (for non-RDP or direct results)
            result
        };

        // Parse the JavaScript result into AXNodes
        let mut nodes = Vec::new();
        let root_id = self.parse_js_tree(&tree_json, &mut nodes)?;

        // Build the tree
        let mut tree = AXTree::new();
        if let Some(root) = root_id {
            tree.update(nodes, root);
        }

        Ok(tree)
    }

    // ========== Input ==========

    async fn focus_node(&self, handle: &NodeHandle) -> BackendResult<()> {
        // For Firefox, we use JavaScript to focus
        let _actor_id = match &handle.inner {
            NodeHandleInner::Firefox { actor_id } => actor_id,
            _ => return Err(BackendError::InvalidHandle("Not a Firefox handle".into())),
        };

        // Use JavaScript to focus the element
        // Note: Firefox accessibility doesn't give us direct focus control
        // We'd need to map the accessibility node back to a DOM element
        self.eval_js_sync("document.activeElement?.blur(); document.body?.focus()")?;

        Ok(())
    }

    async fn click_node(&self, handle: &NodeHandle) -> BackendResult<()> {
        let _actor_id = match &handle.inner {
            NodeHandleInner::Firefox { actor_id } => actor_id,
            _ => return Err(BackendError::InvalidHandle("Not a Firefox handle".into())),
        };

        // For Firefox, clicking accessibility nodes is complex
        // We need to use the DoDefaultAction or simulate via JavaScript
        // This is a simplified implementation
        self.eval_js_sync("document.activeElement?.click()")?;

        Ok(())
    }

    async fn send_text(&self, text: &str) -> BackendResult<()> {
        // Insert text using JavaScript
        let script = format!(
            r#"
            (function() {{
                const el = document.activeElement;
                if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {{
                    el.value += {};
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                }}
            }})()
            "#,
            serde_json::to_string(text).unwrap_or_default()
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn send_key(&self, key: Key, modifiers: KeyModifiers) -> BackendResult<()> {
        let key_info = key_to_js_params(&key);

        let script = format!(
            r#"
            (function() {{
                const el = document.activeElement || document.body;
                const event = new KeyboardEvent('keydown', {{
                    key: '{}',
                    code: '{}',
                    keyCode: {},
                    which: {},
                    ctrlKey: {},
                    shiftKey: {},
                    altKey: {},
                    metaKey: {},
                    bubbles: true
                }});
                el.dispatchEvent(event);

                const upEvent = new KeyboardEvent('keyup', {{
                    key: '{}',
                    code: '{}',
                    bubbles: true
                }});
                el.dispatchEvent(upEvent);
            }})()
            "#,
            key_info.0, key_info.1, key_info.2, key_info.2,
            modifiers.ctrl, modifiers.shift, modifiers.alt, modifiers.meta,
            key_info.0, key_info.1
        );

        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn set_field_value(&self, _handle: &NodeHandle, value: &str) -> BackendResult<()> {
        let script = format!(
            r#"
            (function() {{
                const el = document.activeElement;
                if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA')) {{
                    el.value = {};
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                }}
            }})()
            "#,
            serde_json::to_string(value).unwrap_or_default()
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn get_field_value(&self, _handle: &NodeHandle) -> BackendResult<String> {
        let result = self.eval_js_sync("document.activeElement?.value || ''")?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    // ========== Scrolling ==========

    async fn scroll(&self, delta_x: i32, delta_y: i32) -> BackendResult<()> {
        let script = format!("window.scrollBy({}, {})", delta_x, delta_y);
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn scroll_to_top(&self) -> BackendResult<()> {
        self.eval_js_sync("window.scrollTo(0, 0)")?;
        Ok(())
    }

    async fn scroll_to_bottom(&self) -> BackendResult<()> {
        self.eval_js_sync("window.scrollTo(0, document.body.scrollHeight)")?;
        Ok(())
    }

    // ========== Viewport ==========

    async fn set_viewport(&self, width: u32, height: u32, _mobile: bool) -> BackendResult<()> {
        // Firefox doesn't have an equivalent to CDP's Emulation.setDeviceMetricsOverride
        // We can try to resize the window via JavaScript, but it may not work in headless mode
        let script = format!("window.resizeTo({}, {})", width, height);
        let _ = self.eval_js_sync(&script);
        Ok(())
    }

    // ========== Events ==========

    fn try_recv_event(&self) -> Option<BrowserEvent> {
        self.client.try_recv_event().map(|e| {
            match e.event_type.as_str() {
                "tabNavigated" => BrowserEvent::UrlChanged {
                    url: e.data.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                },
                "documentReady" | "DOMContentLoaded" => BrowserEvent::LoadComplete,
                _ => BrowserEvent::Other {
                    name: e.event_type,
                    data: e.data,
                },
            }
        })
    }

    async fn recv_event(&self) -> Option<BrowserEvent> {
        self.try_recv_event()
    }

    // ========== Media ==========

    async fn has_media(&self) -> BackendResult<bool> {
        let result = self.eval_js_sync("!!document.querySelector('video, audio')")?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn get_media_status(&self) -> BackendResult<MediaStatus> {
        let result = self.eval_js_sync(r#"
            (function() {
                const v = document.querySelector('video') || document.querySelector('audio');
                if (!v) return { has_video: false };
                return {
                    has_video: true,
                    playing: !v.paused,
                    muted: v.muted,
                    volume: v.volume,
                    current_time: v.currentTime,
                    duration: v.duration || 0,
                    playback_rate: v.playbackRate,
                    has_captions: v.textTracks && v.textTracks.length > 0,
                    captions_visible: v.textTracks && v.textTracks[0] && v.textTracks[0].mode === 'showing'
                };
            })()
        "#)?;

        Ok(MediaStatus {
            has_video: result.get("has_video").and_then(|v| v.as_bool()).unwrap_or(false),
            playing: result.get("playing").and_then(|v| v.as_bool()).unwrap_or(false),
            muted: result.get("muted").and_then(|v| v.as_bool()).unwrap_or(false),
            volume: result.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0),
            current_time: result.get("current_time").and_then(|v| v.as_f64()).unwrap_or(0.0),
            duration: result.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
            playback_rate: result.get("playback_rate").and_then(|v| v.as_f64()).unwrap_or(1.0),
            has_captions: result.get("has_captions").and_then(|v| v.as_bool()).unwrap_or(false),
            captions_visible: result.get("captions_visible").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }

    async fn media_toggle_play(&self) -> BackendResult<bool> {
        let result = self.eval_js_sync(r#"
            (function() {
                const v = document.querySelector('video') || document.querySelector('audio');
                if (!v) return false;
                if (v.paused) { v.play(); return true; }
                else { v.pause(); return false; }
            })()
        "#)?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_seek(&self, seconds: f64) -> BackendResult<()> {
        let script = format!(
            "(function() {{ const v = document.querySelector('video') || document.querySelector('audio'); if (v) v.currentTime += {}; }})()",
            seconds
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn media_seek_percent(&self, percent: u8) -> BackendResult<()> {
        let script = format!(
            "(function() {{ const v = document.querySelector('video') || document.querySelector('audio'); if (v) v.currentTime = v.duration * {} / 100; }})()",
            percent
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn media_adjust_volume(&self, delta: f64) -> BackendResult<()> {
        let script = format!(
            "(function() {{ const v = document.querySelector('video') || document.querySelector('audio'); if (v) v.volume = Math.max(0, Math.min(1, v.volume + {})); }})()",
            delta
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    async fn media_toggle_mute(&self) -> BackendResult<bool> {
        let result = self.eval_js_sync(r#"
            (function() {
                const v = document.querySelector('video') || document.querySelector('audio');
                if (!v) return false;
                v.muted = !v.muted;
                return v.muted;
            })()
        "#)?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_toggle_captions(&self) -> BackendResult<bool> {
        let result = self.eval_js_sync(r#"
            (function() {
                const v = document.querySelector('video');
                if (!v || !v.textTracks || v.textTracks.length === 0) return false;
                const track = v.textTracks[0];
                if (track.mode === 'showing') { track.mode = 'hidden'; return false; }
                else { track.mode = 'showing'; return true; }
            })()
        "#)?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_set_speed(&self, speed: f64) -> BackendResult<()> {
        let script = format!(
            "(function() {{ const v = document.querySelector('video') || document.querySelector('audio'); if (v) v.playbackRate = {}; }})()",
            speed
        );
        self.eval_js_sync(&script)?;
        Ok(())
    }

    // ========== JavaScript ==========

    async fn evaluate_js(&self, script: &str) -> BackendResult<Value> {
        self.eval_js_sync(script)
    }

    // ========== Lifecycle ==========

    async fn close(&self) -> BackendResult<()> {
        // Close the tab/target
        // In Firefox, this would involve sending a close message to the tab descriptor
        Ok(())
    }
}

impl FirefoxSession {
    /// Extract a value from either direct value or RDP preview format
    fn extract_value<'a>(obj: &'a Value, key: &str) -> Option<&'a Value> {
        let val = obj.get(key)?;
        // Check if it's in RDP preview format: {"value": ...}
        if let Some(inner) = val.get("value") {
            // Handle null type
            if inner.get("type").and_then(|t| t.as_str()) == Some("null") {
                return None;
            }
            Some(inner)
        } else {
            Some(val)
        }
    }

    /// Extract string from either direct value or RDP preview format
    fn extract_str<'a>(obj: &'a Value, key: &str) -> Option<&'a str> {
        Self::extract_value(obj, key).and_then(|v| v.as_str())
    }

    /// Parse JavaScript tree result into AXNodes
    fn parse_js_tree(
        &self,
        node: &Value,
        nodes: &mut Vec<AXNode>,
    ) -> BackendResult<Option<String>> {
        if node.is_null() || !node.is_object() {
            return Ok(None);
        }

        let id = Self::extract_str(node, "id").unwrap_or("").to_string();
        if id.is_empty() {
            return Ok(None);
        }

        let role_str = Self::extract_str(node, "role").unwrap_or("generic");
        let name = Self::extract_str(node, "name").unwrap_or("").to_string();
        let level = Self::extract_value(node, "level")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;
        let parent_id = Self::extract_str(node, "parentId").map(|s| s.to_string());
        let url = Self::extract_str(node, "url").map(|s| s.to_string());
        let focusable = Self::extract_value(node, "focusable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Convert role
        let role = firefox_role_to_role(role_str);

        // Create AXNode
        let mut ax_node = AXNode::with_handle(
            id.clone(),
            NodeHandle {
                inner: NodeHandleInner::Firefox {
                    actor_id: id.clone(),
                },
            },
        );
        ax_node.role = role;
        ax_node.name = name;
        ax_node.level = level;
        ax_node.parent_id = parent_id;
        ax_node.url = url;
        ax_node.state.focusable = focusable;

        // Note: For nested objects like children and childIds, we can't easily
        // get them from the RDP preview. The root node's children need to be
        // fetched separately. For now, this works for the root node.
        // Child nodes come through the "children" array which has full objects.

        // Get child IDs from either direct or preview format
        if let Some(child_ids_val) = Self::extract_value(node, "childIds") {
            if let Some(arr) = child_ids_val.as_array() {
                ax_node.child_ids = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
        }

        // Process children recursively - check both formats
        let children = node.get("children")
            .and_then(|v| {
                // Direct array format
                if let Some(arr) = v.as_array() {
                    return Some(arr);
                }
                // Preview format: {"value": {"actor": ...}}
                // Can't get array from preview, skip for now
                None
            });

        if let Some(children) = children {
            for child in children {
                self.parse_js_tree(child, nodes)?;
            }
        }

        // Add this node
        nodes.push(ax_node);

        Ok(Some(id))
    }

    /// Collect accessibility nodes recursively into a Vec (native walker - unused in headless)
    /// Returns the root node ID if successful
    #[allow(dead_code)]
    fn collect_nodes(
        &self,
        walker: &str,
        node: &Value,
        parent_id: Option<&str>,
        nodes: &mut Vec<AXNode>,
    ) -> BackendResult<Option<String>> {
        let actor = node.get("actor").and_then(|a| a.as_str()).unwrap_or("");
        if actor.is_empty() {
            return Ok(None);
        }

        // Get node properties
        let role_str = node.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
        let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();

        // Convert role
        let role = firefox_role_to_role(role_str);

        // Create AXNode
        let mut ax_node = AXNode::with_handle(
            actor.to_string(),
            NodeHandle {
                inner: NodeHandleInner::Firefox {
                    actor_id: actor.to_string(),
                },
            },
        );
        ax_node.role = role;
        ax_node.name = name;
        ax_node.parent_id = parent_id.map(|s| s.to_string());

        // Get state from properties
        if let Some(states) = node.get("states").and_then(|s| s.as_array()) {
            for state in states {
                if let Some(state_str) = state.as_str() {
                    match state_str {
                        "focusable" => ax_node.state.focusable = true,
                        "focused" => ax_node.state.focused = true,
                        "selected" => ax_node.state.selected = true,
                        "checked" => ax_node.state.checked = Some(true),
                        "disabled" | "unavailable" => ax_node.state.disabled = true,
                        "readonly" => ax_node.state.readonly = true,
                        "required" => ax_node.state.required = true,
                        "visited" => ax_node.state.visited = true,
                        _ => {}
                    }
                }
            }
        }

        // Get child count
        let child_count = node.get("childCount").and_then(|c| c.as_u64()).unwrap_or(0) as usize;

        // Collect child IDs and process children
        let mut child_ids = Vec::new();
        if child_count > 0 {
            if let Ok(children) = ActorInfo::get_children(&self.client, walker, actor) {
                for child in &children {
                    if let Some(child_actor) = child.get("actor").and_then(|a| a.as_str()) {
                        child_ids.push(child_actor.to_string());
                        // Recursively collect child nodes
                        self.collect_nodes(walker, child, Some(actor), nodes)?;
                    }
                }
            }
        }

        // Set child IDs on this node before adding to the collection
        ax_node.child_ids = child_ids;

        // Add node to collection
        nodes.push(ax_node);

        Ok(Some(actor.to_string()))
    }
}

/// Convert a Firefox role string to our Role enum
fn firefox_role_to_role(role: &str) -> Role {
    match role.to_lowercase().as_str() {
        "document" | "document web" => Role::Document,
        "article" => Role::Article,
        "section" => Role::Section,
        "heading" => Role::Heading,
        "paragraph" | "text container" => Role::Paragraph,
        "text" | "text leaf" | "static text" => Role::StaticText,
        "link" => Role::Link,
        "button" | "pushbutton" => Role::Button,
        "entry" | "text field" | "password text" => Role::TextField,
        "multiline text" => Role::TextFieldMultiLine,
        "check box" | "checkbox" => Role::CheckBox,
        "radio button" | "radiobutton" => Role::RadioButton,
        "combobox" | "combo box" => Role::ComboBox,
        "listbox" | "list box" => Role::ListBox,
        "list" => Role::List,
        "listitem" | "list item" => Role::ListItem,
        "table" => Role::Table,
        "row" | "table row" => Role::Row,
        "cell" | "table cell" => Role::Cell,
        "column header" => Role::ColumnHeader,
        "row header" => Role::RowHeader,
        "banner" | "landmark banner" => Role::Banner,
        "navigation" | "landmark navigation" => Role::Navigation,
        "main" | "landmark main" => Role::Main,
        "content info" | "landmark contentinfo" => Role::ContentInfo,
        "complementary" | "landmark complementary" => Role::Complementary,
        "search" | "landmark search" => Role::Search,
        "form" | "landmark form" => Role::Form,
        "image" | "graphic" => Role::Image,
        "figure" => Role::Figure,
        "grouping" | "group" => Role::Group,
        _ => Role::Generic,
    }
}

/// Convert Key to JavaScript event parameters (key, code, keyCode)
fn key_to_js_params(key: &Key) -> (&'static str, &'static str, u32) {
    match key {
        Key::Enter => ("Enter", "Enter", 13),
        Key::Tab => ("Tab", "Tab", 9),
        Key::Escape => ("Escape", "Escape", 27),
        Key::Backspace => ("Backspace", "Backspace", 8),
        Key::Delete => ("Delete", "Delete", 46),
        Key::ArrowUp => ("ArrowUp", "ArrowUp", 38),
        Key::ArrowDown => ("ArrowDown", "ArrowDown", 40),
        Key::ArrowLeft => ("ArrowLeft", "ArrowLeft", 37),
        Key::ArrowRight => ("ArrowRight", "ArrowRight", 39),
        Key::Home => ("Home", "Home", 36),
        Key::End => ("End", "End", 35),
        Key::PageUp => ("PageUp", "PageUp", 33),
        Key::PageDown => ("PageDown", "PageDown", 34),
        Key::Space => (" ", "Space", 32),
        Key::Char(_) => {
            // For characters, we'd need to handle this dynamically
            // This is a simplification
            (" ", "Key", 0)
        }
        Key::F(_) => {
            // F1-F12
            ("F1", "F1", 112)
        }
    }
}
