//! Chromium page session implementing the PageSession trait.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::cdp_types::CdpAXNode;
use super::client::{CdpClient, CdpEvent};
use crate::accessibility::AXTree;
use crate::backend::{
    BackendError, BackendKind, BackendResult, BrowserEvent, Key, KeyModifiers, MediaStatus,
    NodeHandle, NodeHandleInner, PageSession,
};

pub struct ChromiumSession {
    client: Arc<CdpClient>,
    url: Mutex<String>,
    title: Mutex<String>,
}

impl ChromiumSession {
    pub async fn new(client: CdpClient, initial_url: String) -> BackendResult<Self> {
        let client = Arc::new(client);

        // Enable required domains
        let _ = client.call("Network.enable", json!({})).await;
        let _ = client.call("Page.enable", json!({})).await;
        let _ = client.call("DOM.enable", json!({})).await;
        let _ = client.call("Accessibility.enable", json!({})).await;

        // Get document to start receiving DOM mutation events
        let _ = client.call("DOM.getDocument", json!({ "depth": 0 })).await;

        Ok(Self {
            client,
            url: Mutex::new(initial_url),
            title: Mutex::new(String::new()),
        })
    }

    /// Convert CDP event to BrowserEvent
    fn convert_event(cdp_event: &CdpEvent) -> Option<BrowserEvent> {
        // Log all events for debugging
        if cdp_event.method.starts_with("Accessibility.") || cdp_event.method.starts_with("DOM.") {
            crate::utils::log::log(&format!("[CDP Event] {}", cdp_event.method));
        }

        match cdp_event.method.as_str() {
            "Page.loadEventFired" => Some(BrowserEvent::LoadComplete),
            "Page.frameStartedLoading" => Some(BrowserEvent::LoadStarted),
            "DOM.documentUpdated" | "DOM.childNodeCountUpdated" | "DOM.childNodeInserted"
            | "DOM.childNodeRemoved" | "DOM.attributeModified" => Some(BrowserEvent::DomChanged),
            "Accessibility.loadComplete" | "Accessibility.nodesUpdated" => {
                Some(BrowserEvent::AccessibilityChanged)
            }
            "Network.requestWillBeSent" => {
                let request_id = cdp_event
                    .params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BrowserEvent::NetworkRequestStarted { request_id })
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                let request_id = cdp_event
                    .params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BrowserEvent::NetworkRequestCompleted { request_id })
            }
            "Page.frameNavigated" => {
                let url = cdp_event
                    .params
                    .get("frame")
                    .and_then(|f| f.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(BrowserEvent::UrlChanged { url })
            }
            _ => Some(BrowserEvent::Other {
                name: cdp_event.method.clone(),
                data: cdp_event.params.clone(),
            }),
        }
    }

    /// Get backend DOM node ID from handle
    fn get_backend_node_id(handle: &NodeHandle) -> Option<i64> {
        match &handle.inner {
            NodeHandleInner::Chromium {
                backend_dom_node_id,
                ..
            } => Some(*backend_dom_node_id),
            _ => None,
        }
    }
}

#[async_trait]
impl PageSession for ChromiumSession {
    fn kind(&self) -> BackendKind {
        BackendKind::Chromium
    }

    // ========== Navigation ==========

    async fn navigate(&self, url: &str) -> BackendResult<()> {
        self.client
            .call("Page.navigate", json!({ "url": url }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Navigation failed: {}", e)))?;

        *self.url.lock().await = url.to_string();
        Ok(())
    }

    async fn reload(&self) -> BackendResult<()> {
        self.client
            .call("Page.reload", json!({ "ignoreCache": true }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Reload failed: {}", e)))?;
        Ok(())
    }

    async fn current_url(&self) -> BackendResult<String> {
        let result = self
            .client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": "window.location.href",
                    "returnByValue": true
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        if let Some(url) = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
        {
            let mut guard = self.url.lock().await;
            *guard = url.to_string();
            Ok(url.to_string())
        } else {
            Ok(self.url.lock().await.clone())
        }
    }

    async fn title(&self) -> BackendResult<String> {
        let result = self
            .client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": "document.title",
                    "returnByValue": true
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        if let Some(title) = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
        {
            let mut guard = self.title.lock().await;
            *guard = title.to_string();
            Ok(title.to_string())
        } else {
            Ok(self.title.lock().await.clone())
        }
    }

    // ========== Accessibility ==========

    async fn enable_accessibility(&self) -> BackendResult<()> {
        self.client
            .call("Accessibility.enable", json!({}))
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn get_accessibility_tree(&self) -> BackendResult<AXTree> {
        // Force browser to recalculate layout before fetching accessibility tree
        // This helps ensure CSS visibility changes are reflected
        let _ = self.client.call("Runtime.evaluate", json!({
            "expression": "document.body.offsetHeight",
            "returnByValue": true
        })).await;

        crate::utils::log::log("[DEBUG] Chromium: calling Accessibility.getFullAXTree");
        let result = self
            .client
            .call("Accessibility.getFullAXTree", json!({}))
            .await
            .map_err(|e| {
                crate::utils::log::log(&format!("[DEBUG] Chromium: getFullAXTree failed: {}", e));
                BackendError::Protocol(format!("Failed to get tree: {}", e))
            })?;

        crate::utils::log::log("[DEBUG] Chromium: got result, parsing nodes");
        let cdp_nodes: Vec<CdpAXNode> = serde_json::from_value(
            result.get("nodes").cloned().unwrap_or(Value::Array(vec![])),
        )
        .map_err(|e| BackendError::Protocol(format!("Failed to parse nodes: {}", e)))?;
        crate::utils::log::log(&format!("[DEBUG] Chromium: parsed {} CDP nodes", cdp_nodes.len()));

        // Find "Ouvrir le menu" button and check its expanded state
        for node in &cdp_nodes {
            if node.name_str().contains("menu") || node.name_str().contains("Menu") {
                let expanded = node.get_property("expanded");
                crate::utils::log::log(&format!("[DEBUG] Menu button: {} role={} expanded={:?}",
                    node.name_str(), node.role_str(), expanded));
            }
        }

        // Count images for debugging
        let image_count = cdp_nodes.iter()
            .filter(|n| n.role_str().to_lowercase() == "image" || n.role_str().to_lowercase() == "img")
            .count();
        if image_count > 0 {
            crate::utils::log::log(&format!("[DEBUG] Found {} images in tree", image_count));
        }

        // Convert CDP nodes to unified nodes
        // NOTE: We include ALL nodes (even ignored) to preserve tree structure.
        // Ignored nodes get Role::Generic which makes is_interesting() return false,
        // so they'll be skipped during linearization but tree traversal still works.
        let unified_nodes: Vec<_> = cdp_nodes
            .iter()
            .map(|n| n.to_unified())
            .collect();
        crate::utils::log::log(&format!("[DEBUG] Chromium: {} unified nodes", unified_nodes.len()));

        // Find root ID
        let root_id = cdp_nodes
            .iter()
            .find(|n| n.parent_id.is_none())
            .map(|n| n.node_id.clone())
            .unwrap_or_default();

        crate::utils::log::log(&format!("[DEBUG] Root ID: {:?}", root_id));

        let mut tree = AXTree::new();
        tree.update(unified_nodes, root_id);
        Ok(tree)
    }

    // ========== Input ==========

    async fn focus_node(&self, handle: &NodeHandle) -> BackendResult<()> {
        let backend_id = Self::get_backend_node_id(handle)
            .ok_or_else(|| BackendError::InvalidHandle("Not a Chromium node handle".into()))?;

        self.client
            .call("DOM.focus", json!({ "backendNodeId": backend_id }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Focus failed: {}", e)))?;
        Ok(())
    }

    async fn click_node(&self, handle: &NodeHandle) -> BackendResult<()> {
        let backend_id = Self::get_backend_node_id(handle)
            .ok_or_else(|| BackendError::InvalidHandle("Not a Chromium node handle".into()))?;

        // Debug: Get element info before clicking
        let describe_result = self.client
            .call("DOM.describeNode", json!({ "backendNodeId": backend_id, "depth": 0 }))
            .await;
        if let Ok(desc) = &describe_result {
            if let Some(node) = desc.get("node") {
                let tag = node.get("nodeName").and_then(|v| v.as_str()).unwrap_or("?");
                let attrs = node.get("attributes").and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
                    .unwrap_or_default();
                crate::utils::log::log(&format!("[DEBUG] Click target: <{}> attrs=[{}]", tag, attrs));
            }
        }

        // Scroll element into view first
        let _ = self.client
            .call("DOM.scrollIntoViewIfNeeded", json!({ "backendNodeId": backend_id }))
            .await;

        // Small delay for scroll to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Get content quads (viewport-relative coordinates)
        let quads_result = self.client
            .call("DOM.getContentQuads", json!({ "backendNodeId": backend_id }))
            .await;

        crate::utils::log::log(&format!("[DEBUG] getContentQuads: {:?}", quads_result));

        if let Ok(quads) = quads_result {
            if let Some(first_quad) = quads.get("quads")
                .and_then(|q| q.as_array())
                .and_then(|a| a.first())
                .and_then(|q| q.as_array())
            {
                // Quad is 8 values: [x1,y1,x2,y2,x3,y3,x4,y4]
                let x1 = first_quad.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y1 = first_quad.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let _x3 = first_quad.get(4).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let _y3 = first_quad.get(5).and_then(|v| v.as_f64()).unwrap_or(0.0);

                // Click near top-left corner (offset by 5px) to avoid hitting child elements like icons
                let center_x = x1 + 5.0;
                let center_y = y1 + 5.0;

                crate::utils::log::log(&format!("[DEBUG] Click at viewport ({}, {})", center_x, center_y));

                // Debug: Check what element is at these coordinates
                let element_check = self.client
                    .call("Runtime.evaluate", json!({
                        "expression": format!(
                            "(function() {{ var el = document.elementFromPoint({}, {}); return el ? '<' + el.tagName + '>' + (el.id ? '#'+el.id : '') + (el.className ? '.'+el.className : '') + ' aria-expanded=' + el.getAttribute('aria-expanded') : 'no element'; }})()",
                            center_x, center_y
                        ),
                        "returnByValue": true
                    }))
                    .await;
                if let Ok(result) = element_check {
                    if let Some(value) = result.get("result").and_then(|r| r.get("value")) {
                        crate::utils::log::log(&format!("[DEBUG] Element at point: {:?}", value));
                    }
                }

                // Simple CDP mouse click sequence
                self.client
                    .call("Input.dispatchMouseEvent", json!({
                        "type": "mousePressed",
                        "x": center_x,
                        "y": center_y,
                        "button": "left",
                        "clickCount": 1
                    }))
                    .await
                    .ok();

                self.client
                    .call("Input.dispatchMouseEvent", json!({
                        "type": "mouseReleased",
                        "x": center_x,
                        "y": center_y,
                        "button": "left",
                        "clickCount": 1
                    }))
                    .await
                    .ok();

                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                return Ok(());
            }
        }

        // Fallback: focus and press Space (most reliable for buttons)
        crate::utils::log::log("[DEBUG] getContentQuads failed, using keyboard fallback");
        self.client
            .call("DOM.focus", json!({ "backendNodeId": backend_id }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Focus failed: {}", e)))?;

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Send Space key (works better for buttons/toggles)
        self.client
            .call("Input.dispatchKeyEvent", json!({
                "type": "keyDown",
                "key": " ",
                "code": "Space",
                "windowsVirtualKeyCode": 32
            }))
            .await
            .ok();

        self.client
            .call("Input.dispatchKeyEvent", json!({
                "type": "keyUp",
                "key": " ",
                "code": "Space",
                "windowsVirtualKeyCode": 32
            }))
            .await
            .ok();

        Ok(())
    }

    async fn send_text(&self, text: &str) -> BackendResult<()> {
        self.client
            .call("Input.insertText", json!({ "text": text }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Send text failed: {}", e)))?;
        Ok(())
    }

    async fn send_key(&self, key: Key, modifiers: KeyModifiers) -> BackendResult<()> {
        let (key_str, code, vk) = match key {
            Key::Enter => ("Enter", "Enter", 13),
            Key::Tab => ("Tab", "Tab", 9),
            Key::Backspace => ("Backspace", "Backspace", 8),
            Key::Delete => ("Delete", "Delete", 46),
            Key::Escape => ("Escape", "Escape", 27),
            Key::ArrowUp => ("ArrowUp", "ArrowUp", 38),
            Key::ArrowDown => ("ArrowDown", "ArrowDown", 40),
            Key::ArrowLeft => ("ArrowLeft", "ArrowLeft", 37),
            Key::ArrowRight => ("ArrowRight", "ArrowRight", 39),
            Key::Home => ("Home", "Home", 36),
            Key::End => ("End", "End", 35),
            Key::PageUp => ("PageUp", "PageUp", 33),
            Key::PageDown => ("PageDown", "PageDown", 34),
            Key::Space => (" ", "Space", 32),
            Key::Char(c) => {
                // For character keys, use insertText
                let text = if modifiers.shift {
                    c.to_uppercase().to_string()
                } else {
                    c.to_string()
                };
                self.client
                    .call("Input.insertText", json!({ "text": text }))
                    .await
                    .map_err(|e| BackendError::Protocol(e.to_string()))?;
                return Ok(());
            }
            Key::F(n) => {
                // Handle F-keys separately since we need owned strings
                let key_str = format!("F{}", n);
                let vk = 111 + n as i32; // F1 = 112

                let mut mod_flags = 0;
                if modifiers.alt {
                    mod_flags |= 1;
                }
                if modifiers.ctrl {
                    mod_flags |= 2;
                }
                if modifiers.meta {
                    mod_flags |= 4;
                }
                if modifiers.shift {
                    mod_flags |= 8;
                }

                self.client
                    .call(
                        "Input.dispatchKeyEvent",
                        json!({
                            "type": "keyDown",
                            "key": &key_str,
                            "code": &key_str,
                            "windowsVirtualKeyCode": vk,
                            "nativeVirtualKeyCode": vk,
                            "modifiers": mod_flags
                        }),
                    )
                    .await
                    .map_err(|e| BackendError::Protocol(e.to_string()))?;

                self.client
                    .call(
                        "Input.dispatchKeyEvent",
                        json!({
                            "type": "keyUp",
                            "key": &key_str,
                            "code": &key_str,
                            "windowsVirtualKeyCode": vk,
                            "nativeVirtualKeyCode": vk,
                            "modifiers": mod_flags
                        }),
                    )
                    .await
                    .map_err(|e| BackendError::Protocol(e.to_string()))?;

                return Ok(());
            }
        };

        let mut mod_flags = 0;
        if modifiers.alt {
            mod_flags |= 1;
        }
        if modifiers.ctrl {
            mod_flags |= 2;
        }
        if modifiers.meta {
            mod_flags |= 4;
        }
        if modifiers.shift {
            mod_flags |= 8;
        }

        // Key down
        self.client
            .call(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyDown",
                    "key": key_str,
                    "code": code,
                    "windowsVirtualKeyCode": vk,
                    "nativeVirtualKeyCode": vk,
                    "modifiers": mod_flags
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        // Key up
        self.client
            .call(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyUp",
                    "key": key_str,
                    "code": code,
                    "windowsVirtualKeyCode": vk,
                    "nativeVirtualKeyCode": vk,
                    "modifiers": mod_flags
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(())
    }

    async fn set_field_value(&self, handle: &NodeHandle, value: &str) -> BackendResult<()> {
        let backend_id = Self::get_backend_node_id(handle)
            .ok_or_else(|| BackendError::InvalidHandle("Not a Chromium node handle".into()))?;

        // Focus the element
        self.client
            .call("DOM.focus", json!({ "backendNodeId": backend_id }))
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        // Set value and dispatch events
        let value_json = serde_json::to_string(value).unwrap_or_default();
        self.client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": format!(
                        "if (document.activeElement) {{ document.activeElement.value = {}; document.activeElement.dispatchEvent(new Event('input', {{ bubbles: true }})); }}",
                        value_json
                    )
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(())
    }

    async fn get_field_value(&self, handle: &NodeHandle) -> BackendResult<String> {
        let backend_id = Self::get_backend_node_id(handle)
            .ok_or_else(|| BackendError::InvalidHandle("Not a Chromium node handle".into()))?;

        // Focus the element
        self.client
            .call("DOM.focus", json!({ "backendNodeId": backend_id }))
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        // Get value
        let result = self
            .client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": "document.activeElement?.value || ''",
                    "returnByValue": true
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string())
    }

    // ========== Scrolling ==========

    async fn scroll(&self, delta_x: i32, delta_y: i32) -> BackendResult<()> {
        self.client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": format!("window.scrollBy({}, {})", delta_x, delta_y)
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn scroll_to_top(&self) -> BackendResult<()> {
        self.client
            .call(
                "Runtime.evaluate",
                json!({ "expression": "window.scrollTo(0, 0)" }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;
        Ok(())
    }

    async fn scroll_to_bottom(&self) -> BackendResult<()> {
        self.client
            .call(
                "Runtime.evaluate",
                json!({ "expression": "window.scrollTo(0, document.body.scrollHeight)" }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;
        Ok(())
    }

    // ========== Viewport ==========

    async fn set_viewport(&self, width: u32, height: u32, mobile: bool) -> BackendResult<()> {
        // Set device metrics (viewport size)
        self.client
            .call(
                "Emulation.setDeviceMetricsOverride",
                json!({
                    "width": width,
                    "height": height,
                    "deviceScaleFactor": if mobile { 2 } else { 1 },
                    "mobile": mobile
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        // Set user agent to match mobile/desktop
        let user_agent = if mobile {
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
        } else {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        };
        self.client
            .call(
                "Emulation.setUserAgentOverride",
                json!({ "userAgent": user_agent }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(())
    }

    // ========== Events ==========

    fn try_recv_event(&self) -> Option<BrowserEvent> {
        self.client
            .try_recv_event()
            .and_then(|e| Self::convert_event(&e))
    }

    async fn recv_event(&self) -> Option<BrowserEvent> {
        self.client
            .recv_event()
            .await
            .and_then(|e| Self::convert_event(&e))
    }

    // ========== Media ==========

    async fn has_media(&self) -> BackendResult<bool> {
        let result = self
            .evaluate_js("!!document.querySelector('video, audio')")
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn get_media_status(&self) -> BackendResult<MediaStatus> {
        let result = self
            .evaluate_js(
                r#"
            (function() {
                const v = document.querySelector('video');
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
                    captions_visible: Array.from(v.textTracks || []).some(t => t.mode === 'showing')
                };
            })()
        "#,
            )
            .await?;

        Ok(MediaStatus {
            has_video: result
                .get("has_video")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            playing: result
                .get("playing")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            muted: result
                .get("muted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            volume: result
                .get("volume")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0),
            current_time: result
                .get("current_time")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            duration: result
                .get("duration")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            playback_rate: result
                .get("playback_rate")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0),
            has_captions: result
                .get("has_captions")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            captions_visible: result
                .get("captions_visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    async fn media_toggle_play(&self) -> BackendResult<bool> {
        let result = self
            .evaluate_js(
                r#"
            (function() {
                const v = document.querySelector('video');
                if (!v) return false;
                if (v.paused) { v.play(); return true; }
                else { v.pause(); return false; }
            })()
        "#,
            )
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_seek(&self, seconds: f64) -> BackendResult<()> {
        self.evaluate_js(&format!(
            r#"
            (function() {{
                const v = document.querySelector('video');
                if (v) v.currentTime += {};
            }})()
        "#,
            seconds
        ))
        .await?;
        Ok(())
    }

    async fn media_seek_percent(&self, percent: u8) -> BackendResult<()> {
        self.evaluate_js(&format!(
            r#"
            (function() {{
                const v = document.querySelector('video');
                if (v && v.duration) v.currentTime = v.duration * {} / 100;
            }})()
        "#,
            percent
        ))
        .await?;
        Ok(())
    }

    async fn media_adjust_volume(&self, delta: f64) -> BackendResult<()> {
        self.evaluate_js(&format!(
            r#"
            (function() {{
                const v = document.querySelector('video');
                if (v) v.volume = Math.max(0, Math.min(1, v.volume + {}));
            }})()
        "#,
            delta
        ))
        .await?;
        Ok(())
    }

    async fn media_toggle_mute(&self) -> BackendResult<bool> {
        let result = self
            .evaluate_js(
                r#"
            (function() {
                const v = document.querySelector('video');
                if (!v) return false;
                v.muted = !v.muted;
                return v.muted;
            })()
        "#,
            )
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_toggle_captions(&self) -> BackendResult<bool> {
        let result = self
            .evaluate_js(
                r#"
            (function() {
                const v = document.querySelector('video');
                if (!v || !v.textTracks || v.textTracks.length === 0) return false;
                const track = v.textTracks[0];
                if (track.mode === 'showing') {
                    track.mode = 'hidden';
                    return false;
                } else {
                    track.mode = 'showing';
                    return true;
                }
            })()
        "#,
            )
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    async fn media_set_speed(&self, speed: f64) -> BackendResult<()> {
        self.evaluate_js(&format!(
            r#"
            (function() {{
                const v = document.querySelector('video');
                if (v) v.playbackRate = {};
            }})()
        "#,
            speed
        ))
        .await?;
        Ok(())
    }

    // ========== JavaScript ==========

    async fn evaluate_js(&self, script: &str) -> BackendResult<Value> {
        let result = self
            .client
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": script,
                    "returnByValue": true
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn set_document_content(&self, html: &str) -> BackendResult<()> {
        // Get the frame ID first
        let frame_tree = self
            .client
            .call("Page.getFrameTree", json!({}))
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        let frame_id = frame_tree
            .get("frameTree")
            .and_then(|ft| ft.get("frame"))
            .and_then(|f| f.get("id"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| BackendError::Protocol("Failed to get frame ID".to_string()))?;

        // Set document content
        self.client
            .call(
                "Page.setDocumentContent",
                json!({
                    "frameId": frame_id,
                    "html": html
                }),
            )
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;

        Ok(())
    }

    // ========== Lifecycle ==========

    async fn close(&self) -> BackendResult<()> {
        // Close the target/page
        self.client
            .call("Target.closeTarget", json!({}))
            .await
            .map_err(|e| BackendError::Protocol(e.to_string()))?;
        Ok(())
    }
}
