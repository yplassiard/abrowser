//! Configuration file support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::backend::BackendKind;

/// Browser configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Default search engine URL (use %s for query)
    pub search_engine: String,

    /// Viewport mode: "mobile" or "desktop"
    pub viewport_mode: ViewportMode,

    /// Browser backend: "chromium" or "firefox"
    #[serde(with = "backend_serde")]
    pub backend: BackendKind,

    /// Custom profile path (None = use abrowser's own profile)
    /// Set to "system" to use system Chrome profile, or a custom path
    pub profile_path: Option<String>,

    /// Key bindings for navigation mode
    pub navigation_keys: KeyBindings,

    /// Key bindings for focus mode
    pub focus_keys: KeyBindings,

    /// Output settings
    pub output: OutputConfig,

    /// AI image description settings
    pub ai: AiConfig,
}

/// Serde helper for BackendKind
mod backend_serde {
    use super::BackendKind;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(kind: &BackendKind, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&kind.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BackendKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Viewport mode for rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ViewportMode {
    /// Mobile viewport (375x812)
    Mobile,
    /// Desktop viewport (1920x1080)
    #[default]
    Desktop,
}

impl ViewportMode {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            ViewportMode::Mobile => (375, 812),
            ViewportMode::Desktop => (1920, 1080),
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            ViewportMode::Mobile => ViewportMode::Desktop,
            ViewportMode::Desktop => ViewportMode::Mobile,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ViewportMode::Mobile => "mobile",
            ViewportMode::Desktop => "desktop",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            search_engine: "https://duckduckgo.com/?q=%s".to_string(),
            viewport_mode: ViewportMode::default(),
            backend: BackendKind::Chromium,
            profile_path: None,
            navigation_keys: KeyBindings::default_navigation(),
            focus_keys: KeyBindings::default_focus(),
            output: OutputConfig::default(),
            ai: AiConfig::default(),
        }
    }
}

impl Config {
    /// Get the browser profile directory path
    pub fn get_profile_path(&self) -> PathBuf {
        match &self.profile_path {
            Some(path) if path == "system" => {
                // Use system Chrome profile
                #[cfg(target_os = "macos")]
                {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("Library/Application Support/Google/Chrome")
                }
                #[cfg(target_os = "linux")]
                {
                    dirs::config_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("google-chrome")
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    // Fallback to abrowser profile
                    dirs::data_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join("abrowser")
                        .join("chrome-profile")
                }
            }
            Some(custom_path) => {
                // Use custom path
                PathBuf::from(custom_path)
            }
            None => {
                // Default abrowser profile
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("abrowser")
                    .join("chrome-profile")
            }
        }
    }

    /// Load config from file, or return default if not found
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str(&contents) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let contents = toml::to_string_pretty(self)?;
            fs::write(path, contents)?;
        }
        Ok(())
    }

    /// Get the config file path
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("abrowser").join("config.toml"))
    }
}

/// Key bindings configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeyBindings {
    /// Custom key bindings (key description -> action)
    pub bindings: HashMap<String, String>,
}

impl KeyBindings {
    pub fn default_navigation() -> Self {
        let mut bindings = HashMap::new();

        // Cursor movement
        bindings.insert("up".to_string(), "cursor_up".to_string());
        bindings.insert("down".to_string(), "cursor_down".to_string());
        bindings.insert("home".to_string(), "line_start".to_string());
        bindings.insert("end".to_string(), "line_end".to_string());
        bindings.insert("pageup".to_string(), "page_up".to_string());
        bindings.insert("pagedown".to_string(), "page_down".to_string());
        bindings.insert("ctrl+home".to_string(), "doc_start".to_string());
        bindings.insert("ctrl+end".to_string(), "doc_end".to_string());

        // Element navigation
        bindings.insert("h".to_string(), "next_heading".to_string());
        bindings.insert("H".to_string(), "prev_heading".to_string());
        bindings.insert("k".to_string(), "next_link".to_string());
        bindings.insert("K".to_string(), "prev_link".to_string());
        bindings.insert("b".to_string(), "next_button".to_string());
        bindings.insert("B".to_string(), "prev_button".to_string());
        bindings.insert("e".to_string(), "next_edit".to_string());
        bindings.insert("E".to_string(), "prev_edit".to_string());
        bindings.insert("x".to_string(), "next_checkbox".to_string());
        bindings.insert("X".to_string(), "prev_checkbox".to_string());
        bindings.insert("r".to_string(), "next_radio".to_string());
        bindings.insert("R".to_string(), "prev_radio".to_string());
        bindings.insert("l".to_string(), "next_list".to_string());
        bindings.insert("L".to_string(), "prev_list".to_string());
        bindings.insert("t".to_string(), "next_table".to_string());
        bindings.insert("T".to_string(), "prev_table".to_string());
        bindings.insert("v".to_string(), "next_visited".to_string());
        bindings.insert("V".to_string(), "prev_visited".to_string());
        bindings.insert("d".to_string(), "next_landmark".to_string());
        bindings.insert("D".to_string(), "prev_landmark".to_string());
        bindings.insert("i".to_string(), "next_image".to_string());
        bindings.insert("I".to_string(), "prev_image".to_string());
        bindings.insert("g".to_string(), "describe_image".to_string());
        bindings.insert("alt+i".to_string(), "describe_image".to_string());

        // Actions
        bindings.insert("enter".to_string(), "activate".to_string());
        bindings.insert("space".to_string(), "toggle".to_string());

        // Browser controls
        bindings.insert("ctrl+l".to_string(), "address_bar".to_string());
        bindings.insert("ctrl+t".to_string(), "new_tab".to_string());
        bindings.insert("ctrl+w".to_string(), "close_tab".to_string());
        bindings.insert("ctrl+o".to_string(), "open_file".to_string());
        bindings.insert("ctrl+s".to_string(), "save_page".to_string());
        bindings.insert("ctrl+p".to_string(), "print".to_string());
        bindings.insert("ctrl+j".to_string(), "downloads".to_string());
        bindings.insert("ctrl+m".to_string(), "toggle_viewport".to_string());
        bindings.insert("ctrl+q".to_string(), "quit".to_string());
        bindings.insert("ctrl+space".to_string(), "toggle_mode".to_string());

        // Tab switching
        bindings.insert("ctrl+0".to_string(), "switch_tab_0".to_string());
        bindings.insert("ctrl+1".to_string(), "switch_tab_1".to_string());
        bindings.insert("ctrl+2".to_string(), "switch_tab_2".to_string());
        bindings.insert("ctrl+3".to_string(), "switch_tab_3".to_string());
        bindings.insert("ctrl+4".to_string(), "switch_tab_4".to_string());
        bindings.insert("ctrl+5".to_string(), "switch_tab_5".to_string());
        bindings.insert("ctrl+6".to_string(), "switch_tab_6".to_string());
        bindings.insert("ctrl+7".to_string(), "switch_tab_7".to_string());
        bindings.insert("ctrl+8".to_string(), "switch_tab_8".to_string());
        bindings.insert("ctrl+9".to_string(), "switch_tab_9".to_string());

        Self { bindings }
    }

    pub fn default_focus() -> Self {
        let mut bindings = HashMap::new();

        // Only mode toggle works in focus mode
        bindings.insert("ctrl+space".to_string(), "toggle_mode".to_string());
        bindings.insert("ctrl+q".to_string(), "quit".to_string());

        Self { bindings }
    }
}

/// Output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Show role prefixes (e.g., "Link:", "Button:")
    pub show_roles: bool,

    /// Braille display width
    pub braille_cells: u8,

    /// Use colors in output
    pub colors: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            show_roles: true,
            braille_cells: 40,
            colors: true,
        }
    }
}

/// AI image description configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Enable AI image descriptions (requires Ollama)
    pub enabled: bool,

    /// Ollama endpoint URL
    pub ollama_endpoint: String,

    /// Model to use for image description
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ollama_endpoint: "http://localhost:11434".to_string(),
            model: "llava".to_string(),
        }
    }
}
