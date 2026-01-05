//! Common types for browser backends.

use std::fmt;

/// Identifies which backend is in use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Chromium,
    Firefox,
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendKind::Chromium => write!(f, "chromium"),
            BackendKind::Firefox => write!(f, "firefox"),
        }
    }
}

impl std::str::FromStr for BackendKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "chromium" | "chrome" => Ok(BackendKind::Chromium),
            "firefox" | "gecko" => Ok(BackendKind::Firefox),
            _ => Err(format!("Unknown backend: {}. Use 'chromium' or 'firefox'", s)),
        }
    }
}

/// Handle to a node that works across backends.
/// Used for click, focus, and other operations that need to reference a specific node.
#[derive(Debug, Clone)]
pub struct NodeHandle {
    pub(crate) inner: NodeHandleInner,
}

/// Internal node handle storage - visible to all backend implementations
#[derive(Debug, Clone)]
pub enum NodeHandleInner {
    Chromium {
        backend_dom_node_id: i64,
        node_id: String,
    },
    Firefox {
        actor_id: String,
    },
}

impl NodeHandle {
    /// Create a Chromium node handle
    pub fn chromium(backend_dom_node_id: i64, node_id: String) -> Self {
        Self {
            inner: NodeHandleInner::Chromium {
                backend_dom_node_id,
                node_id,
            },
        }
    }

    /// Create a Firefox node handle
    pub fn firefox(actor_id: String) -> Self {
        Self {
            inner: NodeHandleInner::Firefox { actor_id },
        }
    }

    /// Get the backend kind for this handle
    pub fn kind(&self) -> BackendKind {
        match &self.inner {
            NodeHandleInner::Chromium { .. } => BackendKind::Chromium,
            NodeHandleInner::Firefox { .. } => BackendKind::Firefox,
        }
    }
}

/// Unified browser events from any backend
#[derive(Debug, Clone)]
pub enum BrowserEvent {
    /// Page started loading
    LoadStarted,
    /// Page finished loading
    LoadComplete,
    /// DOM content loaded (before full load)
    DomContentLoaded,
    /// DOM structure changed (requires tree refresh)
    DomChanged,
    /// Accessibility tree changed
    AccessibilityChanged,
    /// Network request started
    NetworkRequestStarted { request_id: String },
    /// Network request completed successfully
    NetworkRequestCompleted { request_id: String },
    /// Network request failed
    NetworkRequestFailed { request_id: String },
    /// Page title changed
    TitleChanged { title: String },
    /// URL changed
    UrlChanged { url: String },
    /// Focus changed to a different element
    FocusChanged { node_id: Option<String> },
    /// Console message from page
    ConsoleMessage { level: String, text: String },
    /// Unknown/backend-specific event (for extensibility)
    Other { name: String, data: serde_json::Value },
}

/// Backend error type
#[derive(Debug)]
pub enum BackendError {
    /// Connection error (WebSocket, TCP, etc.)
    Connection(String),
    /// Protocol error (invalid response, unexpected format)
    Protocol(String),
    /// Operation not supported by this backend
    NotSupported(String),
    /// Operation timed out
    Timeout,
    /// Node not found (for click/focus operations)
    NodeNotFound,
    /// Navigation failed
    NavigationFailed(String),
    /// JavaScript evaluation error
    JsError(String),
    /// Browser not connected yet
    NotConnected,
    /// Invalid node handle (wrong backend type)
    InvalidHandle(String),
    /// Failed to launch browser
    LaunchFailed(String),
    /// Generic error
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::Connection(msg) => write!(f, "Connection error: {}", msg),
            BackendError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            BackendError::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            BackendError::Timeout => write!(f, "Operation timed out"),
            BackendError::NodeNotFound => write!(f, "Node not found"),
            BackendError::NavigationFailed(msg) => write!(f, "Navigation failed: {}", msg),
            BackendError::JsError(msg) => write!(f, "JavaScript error: {}", msg),
            BackendError::NotConnected => write!(f, "Browser not connected"),
            BackendError::InvalidHandle(msg) => write!(f, "Invalid handle: {}", msg),
            BackendError::LaunchFailed(msg) => write!(f, "Launch failed: {}", msg),
            BackendError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for BackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackendError::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

/// Backend-agnostic result type
pub type BackendResult<T> = Result<T, BackendError>;

/// Keyboard key for input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Char(char),
    F(u8),
}

/// Key modifiers
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl KeyModifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Default::default()
        }
    }

    pub fn shift() -> Self {
        Self {
            shift: true,
            ..Default::default()
        }
    }

    pub fn alt() -> Self {
        Self {
            alt: true,
            ..Default::default()
        }
    }
}

/// Media playback status
#[derive(Debug, Clone, Default)]
pub struct MediaStatus {
    /// Whether the page has video element(s)
    pub has_video: bool,
    /// Whether media is currently playing
    pub playing: bool,
    /// Whether audio is muted
    pub muted: bool,
    /// Volume level (0.0 to 1.0)
    pub volume: f64,
    /// Current playback position in seconds
    pub current_time: f64,
    /// Total duration in seconds
    pub duration: f64,
    /// Playback speed multiplier (1.0 = normal)
    pub playback_rate: f64,
    /// Whether captions are available
    pub has_captions: bool,
    /// Whether captions are currently visible
    pub captions_visible: bool,
}

impl MediaStatus {
    /// Format time as MM:SS or HH:MM:SS
    pub fn format_time(seconds: f64) -> String {
        let total = seconds as u64;
        let hours = total / 3600;
        let mins = (total % 3600) / 60;
        let secs = total % 60;

        if hours > 0 {
            format!("{}:{:02}:{:02}", hours, mins, secs)
        } else {
            format!("{}:{:02}", mins, secs)
        }
    }
}
