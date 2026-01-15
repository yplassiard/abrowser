//! UI module for browser dialogs and options
//!
//! Uses HTML/JS for rich UI, served locally and displayed in a browser tab.

pub mod help;
pub mod options;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// UI server state
pub struct UiServer {
    /// Port the server is running on
    port: u16,
    /// Registered pages
    pages: Arc<RwLock<HashMap<String, String>>>,
}

impl UiServer {
    /// Create a new UI server (doesn't start serving yet)
    pub fn new() -> Self {
        Self {
            port: 0,
            pages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a page
    pub async fn register_page(&self, path: &str, content: String) {
        self.pages.write().await.insert(path.to_string(), content);
    }

    /// Get the options page URL
    pub fn options_url(&self) -> String {
        if self.port > 0 {
            format!("http://localhost:{}/options", self.port)
        } else {
            // Fallback to data URL
            "about:blank".to_string()
        }
    }
}

impl Default for UiServer {
    fn default() -> Self {
        Self::new()
    }
}
