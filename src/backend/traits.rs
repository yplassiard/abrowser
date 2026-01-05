//! Core traits for browser backends.
//!
//! These traits define the interface that both Chromium and Firefox backends
//! must implement, allowing the shell and other components to work with any backend.

use async_trait::async_trait;

use crate::accessibility::AXTree;
use super::types::{
    BackendKind, BackendResult, BrowserEvent, Key, KeyModifiers, MediaStatus, NodeHandle,
};

/// Browser launcher - manages browser process lifecycle.
///
/// Each backend implements this to handle browser-specific launching,
/// such as finding the binary, setting up profiles, and managing the process.
#[async_trait]
pub trait BrowserLauncher: Send + Sync {
    /// Get the backend kind
    fn kind(&self) -> BackendKind;

    /// Launch the browser process.
    ///
    /// This should start the browser in headless mode with remote debugging enabled.
    /// After this call, `create_page()` can be used to open pages.
    async fn launch(&mut self) -> BackendResult<()>;

    /// Create a new page/tab and return a session.
    ///
    /// The URL can be empty or "about:blank" for a blank page.
    async fn create_page(&self, url: &str) -> BackendResult<Box<dyn PageSession>>;

    /// Set the default viewport for new pages.
    fn set_viewport(&mut self, width: u32, height: u32);

    /// Shutdown the browser process gracefully.
    async fn shutdown(&mut self) -> BackendResult<()>;

    /// Check if the browser is still running
    fn is_running(&self) -> bool;
}

/// A page/tab session - main interaction point for browser operations.
///
/// This trait provides all the operations needed to interact with a web page:
/// navigation, accessibility tree access, input, scrolling, and events.
#[async_trait]
pub trait PageSession: Send + Sync {
    /// Get the backend kind
    fn kind(&self) -> BackendKind;

    // ========== Navigation ==========

    /// Navigate to a URL.
    ///
    /// This initiates navigation but may return before the page is fully loaded.
    /// Use events or `wait_for_load()` to detect when loading is complete.
    async fn navigate(&self, url: &str) -> BackendResult<()>;

    /// Reload the current page.
    async fn reload(&self) -> BackendResult<()>;

    /// Get the current URL.
    async fn current_url(&self) -> BackendResult<String>;

    /// Get the page title.
    async fn title(&self) -> BackendResult<String>;

    // ========== Accessibility ==========

    /// Enable accessibility features and event monitoring.
    ///
    /// This should be called before `get_accessibility_tree()` to ensure
    /// the accessibility tree is available.
    async fn enable_accessibility(&self) -> BackendResult<()>;

    /// Get the full accessibility tree.
    ///
    /// Returns a unified `AXTree` that works the same regardless of backend.
    async fn get_accessibility_tree(&self) -> BackendResult<AXTree>;

    // ========== Input ==========

    /// Focus a node by its handle.
    ///
    /// The handle must be from the same backend (Chromium handle for Chromium session, etc.)
    async fn focus_node(&self, handle: &NodeHandle) -> BackendResult<()>;

    /// Click a node.
    ///
    /// This performs a click action on the element, which may trigger navigation
    /// or other JavaScript events.
    async fn click_node(&self, handle: &NodeHandle) -> BackendResult<()>;

    /// Send text input to the currently focused element.
    async fn send_text(&self, text: &str) -> BackendResult<()>;

    /// Send a key event to the page.
    async fn send_key(&self, key: Key, modifiers: KeyModifiers) -> BackendResult<()>;

    /// Set a form field's value directly.
    ///
    /// This sets the value without simulating individual keystrokes.
    /// Also triggers appropriate events (input, change) for reactivity.
    async fn set_field_value(&self, handle: &NodeHandle, value: &str) -> BackendResult<()>;

    /// Get the current value of a form field.
    async fn get_field_value(&self, handle: &NodeHandle) -> BackendResult<String>;

    // ========== Scrolling ==========

    /// Scroll the viewport by a delta.
    async fn scroll(&self, delta_x: i32, delta_y: i32) -> BackendResult<()>;

    /// Scroll to the top of the page.
    async fn scroll_to_top(&self) -> BackendResult<()>;

    /// Scroll to the bottom of the page.
    async fn scroll_to_bottom(&self) -> BackendResult<()>;

    // ========== Viewport ==========

    /// Set the viewport size.
    async fn set_viewport(&self, width: u32, height: u32, mobile: bool) -> BackendResult<()>;

    // ========== Events ==========

    /// Poll for the next event (non-blocking).
    ///
    /// Returns `None` if no event is available.
    fn try_recv_event(&self) -> Option<BrowserEvent>;

    /// Wait for the next event (blocking).
    ///
    /// Returns `None` if the session is closed.
    async fn recv_event(&self) -> Option<BrowserEvent>;

    // ========== Media ==========

    /// Check if the page has media elements.
    async fn has_media(&self) -> BackendResult<bool>;

    /// Get media playback status.
    async fn get_media_status(&self) -> BackendResult<MediaStatus>;

    /// Toggle play/pause on the first media element.
    async fn media_toggle_play(&self) -> BackendResult<bool>;

    /// Seek the media by a number of seconds (positive or negative).
    async fn media_seek(&self, seconds: f64) -> BackendResult<()>;

    /// Seek to a percentage of the media duration.
    async fn media_seek_percent(&self, percent: u8) -> BackendResult<()>;

    /// Adjust the volume (delta: -1.0 to 1.0).
    async fn media_adjust_volume(&self, delta: f64) -> BackendResult<()>;

    /// Toggle mute.
    async fn media_toggle_mute(&self) -> BackendResult<bool>;

    /// Toggle captions/subtitles.
    async fn media_toggle_captions(&self) -> BackendResult<bool>;

    /// Set playback speed.
    async fn media_set_speed(&self, speed: f64) -> BackendResult<()>;

    // ========== JavaScript ==========

    /// Evaluate JavaScript and return the result.
    ///
    /// This is a low-level escape hatch for operations not covered by other methods.
    async fn evaluate_js(&self, script: &str) -> BackendResult<serde_json::Value>;

    // ========== Lifecycle ==========

    /// Close this page session.
    async fn close(&self) -> BackendResult<()>;
}
