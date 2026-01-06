//! FFI bridge between Chromium C++ and Rust.
//!
//! These functions are called from the headless_shell C++ code.

use std::ffi::{c_char, c_int, c_void, CStr};

use crate::shell::Config;

/// Opaque bridge handle for C++
pub struct AbrowserBridge {
    config: Config,
}

#[repr(C)]
pub struct AbrowserSize {
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
pub struct AbrowserBrowserDelegate {
    pub shutdown: Option<extern "C" fn()>,
    pub refresh: Option<extern "C" fn()>,
    pub go_to: Option<extern "C" fn(*const c_char)>,
    pub go_back: Option<extern "C" fn()>,
    pub go_forward: Option<extern "C" fn()>,
    pub scroll: Option<extern "C" fn(c_int)>,
    pub key_press: Option<extern "C" fn(c_char)>,
    pub mouse_down: Option<extern "C" fn(u32, u32)>,
    pub mouse_up: Option<extern "C" fn(u32, u32)>,
    pub mouse_move: Option<extern "C" fn(u32, u32)>,
    pub post_task: Option<extern "C" fn(extern "C" fn(*mut c_void), *mut c_void)>,
}

#[repr(C)]
pub struct AbrowserAxNode {
    pub id: i32,
    pub role: i32,
    pub name: *const c_char,
    pub description: *const c_char,
    pub value: *const c_char,
    pub url: *const c_char,
    pub level: u8,
    pub focusable: bool,
    pub focused: bool,
    pub parent_id: i32,
    pub child_ids: *const i32,
    pub child_count: u32,
}

/// Called early in main() to initialize terminal UI
#[no_mangle]
pub extern "C" fn abrowser_main() {
    // Initialize terminal - for now just a stub
    // The actual initialization happens in the standalone abrowser binary
}

/// Get output mode from config (0=tty, 1=speech, 2=braille)
#[no_mangle]
pub extern "C" fn abrowser_get_output_mode(_bridge: *mut AbrowserBridge) -> c_int {
    // Default to TTY mode
    0
}

/// Get braille cell count from config
#[no_mangle]
pub extern "C" fn abrowser_get_braille_cells(bridge: *mut AbrowserBridge) -> c_int {
    if bridge.is_null() {
        return 40;
    }
    let bridge = unsafe { &*bridge };
    bridge.config.output.braille_cells as c_int
}

/// Get debug mode from config
#[no_mangle]
pub extern "C" fn abrowser_get_debug(_bridge: *mut AbrowserBridge) -> bool {
    // Default to no debug
    false
}

/// Create a new bridge instance
#[no_mangle]
pub extern "C" fn abrowser_bridge_create() -> *mut AbrowserBridge {
    let config = Config::default();
    let bridge = Box::new(AbrowserBridge { config });
    Box::into_raw(bridge)
}

/// Destroy the bridge instance
#[no_mangle]
pub extern "C" fn abrowser_bridge_destroy(bridge: *mut AbrowserBridge) {
    if !bridge.is_null() {
        unsafe {
            drop(Box::from_raw(bridge));
        }
    }
}

/// Start the renderer
#[no_mangle]
pub extern "C" fn abrowser_bridge_start(_bridge: *mut AbrowserBridge) {
    // Stub - shell management happens elsewhere
}

/// Get terminal size
#[no_mangle]
pub extern "C" fn abrowser_bridge_get_size(_bridge: *mut AbrowserBridge) -> AbrowserSize {
    // Get terminal size
    if let Some((width, height)) = term_size::dimensions() {
        AbrowserSize {
            width: width as u32,
            height: height as u32,
        }
    } else {
        AbrowserSize {
            width: 80,
            height: 24,
        }
    }
}

/// Handle terminal resize
#[no_mangle]
pub extern "C" fn abrowser_bridge_resize(_bridge: *mut AbrowserBridge) {
    // Stub - resize handling is done by the C++ side
}

/// Listen for input events (blocking)
#[no_mangle]
pub extern "C" fn abrowser_bridge_listen(
    _bridge: *mut AbrowserBridge,
    delegate: *const AbrowserBrowserDelegate,
) {
    if delegate.is_null() {
        return;
    }

    let delegate = unsafe { &*delegate };

    // Simple input loop using crossterm
    use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
    use crossterm::terminal;

    // Enable raw mode
    if terminal::enable_raw_mode().is_err() {
        return;
    }

    loop {
        if let Ok(Event::Key(KeyEvent {
            code, modifiers, ..
        })) = event::read()
        {
            match (code, modifiers) {
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    // Ctrl+C - shutdown
                    if let Some(shutdown) = delegate.shutdown {
                        shutdown();
                    }
                    break;
                }
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    // Ctrl+R - refresh
                    if let Some(refresh) = delegate.refresh {
                        refresh();
                    }
                }
                (KeyCode::Backspace, KeyModifiers::NONE) => {
                    if let Some(go_back) = delegate.go_back {
                        go_back();
                    }
                }
                (KeyCode::Up, KeyModifiers::NONE) => {
                    if let Some(scroll) = delegate.scroll {
                        scroll(-50);
                    }
                }
                (KeyCode::Down, KeyModifiers::NONE) => {
                    if let Some(scroll) = delegate.scroll {
                        scroll(50);
                    }
                }
                (KeyCode::PageUp, KeyModifiers::NONE) => {
                    if let Some(scroll) = delegate.scroll {
                        scroll(-500);
                    }
                }
                (KeyCode::PageDown, KeyModifiers::NONE) => {
                    if let Some(scroll) = delegate.scroll {
                        scroll(500);
                    }
                }
                (KeyCode::Char(c), KeyModifiers::NONE) => {
                    if let Some(key_press) = delegate.key_press {
                        key_press(c as c_char);
                    }
                }
                _ => {}
            }
        }
    }

    let _ = terminal::disable_raw_mode();
}

/// Push navigation state
#[no_mangle]
pub extern "C" fn abrowser_push_nav(
    _bridge: *mut AbrowserBridge,
    url: *const c_char,
    _can_go_back: bool,
    _can_go_forward: bool,
) {
    if url.is_null() {
        return;
    }

    let url_str = unsafe { CStr::from_ptr(url) };
    if let Ok(url) = url_str.to_str() {
        eprintln!("[abrowser] Navigating to: {}", url);
    }
}

/// Set page title
#[no_mangle]
pub extern "C" fn abrowser_set_title(_bridge: *mut AbrowserBridge, title: *const c_char) {
    if title.is_null() {
        return;
    }

    let title_str = unsafe { CStr::from_ptr(title) };
    if let Ok(title) = title_str.to_str() {
        eprintln!("[abrowser] Title: {}", title);
    }
}

/// Update accessibility tree
#[no_mangle]
pub extern "C" fn abrowser_update_tree(
    _bridge: *mut AbrowserBridge,
    nodes: *const AbrowserAxNode,
    node_count: u32,
    root_id: i32,
) {
    if nodes.is_null() || node_count == 0 {
        return;
    }

    eprintln!(
        "[abrowser] Received {} accessibility nodes, root={}",
        node_count, root_id
    );
}

/// Set focused node
#[no_mangle]
pub extern "C" fn abrowser_set_focus(_bridge: *mut AbrowserBridge, node_id: i32) {
    eprintln!("[abrowser] Focus set to node {}", node_id);
}

/// Announce text for screen reader
#[no_mangle]
pub extern "C" fn abrowser_announce(_bridge: *mut AbrowserBridge, text: *const c_char) {
    if text.is_null() {
        return;
    }

    let text_str = unsafe { CStr::from_ptr(text) };
    if let Ok(text) = text_str.to_str() {
        eprintln!("[abrowser] Announce: {}", text);
    }
}
