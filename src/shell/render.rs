//! Terminal rendering for the browser shell

use super::{BrowserState, Config, FocusMode};
use crossterm::{
    cursor::{self, MoveTo},
    execute,
    style::Print,
    terminal,
};
use std::cell::RefCell;
use std::io::{self, Write};

thread_local! {
    /// Previous frame buffer for double buffering
    static PREV_FRAME: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

/// Render the browser state to the terminal with double buffering
pub fn render(state: &BrowserState, _config: &Config) -> io::Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size()?;
    let width = width as usize;

    // Build new frame in memory
    let mut frame: Vec<String> = Vec::with_capacity(height as usize);

    // Line 0: Tab bar
    frame.push(build_tab_bar(state, width));

    // Lines 1 to height-3: Content area
    let content_height = height.saturating_sub(3) as usize;
    let (content_lines, cursor_line) = build_content(state, width, content_height);
    frame.extend(content_lines);

    // Line height-2: Input prompt or empty
    frame.push(build_input_line(state, width));

    // Line height-1: Status bar
    frame.push(build_status_bar(state, width));

    // Compare with previous frame and only update changed lines
    PREV_FRAME.with(|prev| {
        let mut prev = prev.borrow_mut();

        for (line_num, new_line) in frame.iter().enumerate() {
            let needs_update = prev.get(line_num).map_or(true, |old| old != new_line);
            if needs_update {
                let _ = execute!(stdout, MoveTo(0, line_num as u16));
                let _ = execute!(stdout, Print(new_line));
            }
        }

        // Store current frame for next comparison
        *prev = frame;
    });

    // Position cursor
    let cursor_pos = if state.input_prompt.is_some() {
        let prompt_len = state.input_prompt.as_ref().map_or(0, |p| p.len());
        let cursor_char_pos = state.input_value[..state.input_cursor].chars().count();
        ((prompt_len + cursor_char_pos) as u16, height - 2)
    } else {
        let safe_cursor_line = cursor_line.max(1).min(height - 2);
        (0, safe_cursor_line)
    };

    execute!(stdout, cursor::Show, MoveTo(cursor_pos.0, cursor_pos.1))?;
    stdout.flush()
}

/// Build tab bar line (returns plain string with ANSI codes)
fn build_tab_bar(state: &BrowserState, width: usize) -> String {
    let tab = state.current_tab();

    let tab_num = format!("[{}]", state.current_tab_index);
    let mode = match state.focus_mode {
        FocusMode::Navigation => "[NAV]",
        FocusMode::Focus => "[FOC]",
    };

    let title = if tab.title.is_empty() {
        if tab.url.is_empty() { "New Tab" } else { &tab.url }
    } else {
        &tab.title
    };

    let prefix_len = tab_num.len() + 1 + mode.len() + 1;
    let max_title_len = width.saturating_sub(prefix_len + 1);
    let display_title: String = title.chars().take(max_title_len).collect();

    let used = prefix_len + display_title.chars().count();
    let padding = width.saturating_sub(used);

    format!(
        "\x1b[48;5;240m\x1b[33m{}\x1b[0m\x1b[48;5;240m \x1b[36m{}\x1b[0m\x1b[48;5;240m \x1b[37m{}{}\x1b[0m",
        tab_num, mode, display_title, " ".repeat(padding)
    )
}

/// Build input line
fn build_input_line(state: &BrowserState, width: usize) -> String {
    if let Some(ref prompt) = state.input_prompt {
        let content = format!("{}{}", prompt, state.input_value);
        let padding = width.saturating_sub(content.chars().count());
        format!("\x1b[44m\x1b[37m{}{}\x1b[0m", content, " ".repeat(padding))
    } else {
        " ".repeat(width)
    }
}

/// Build status bar line
fn build_status_bar(state: &BrowserState, width: usize) -> String {
    let tab = state.current_tab();

    let position = if tab.node_count() > 0 {
        format!("{}/{}", tab.cursor_index + 1, tab.node_count())
    } else {
        "0/0".to_string()
    };

    let media_info = if let Some(ref status) = state.media_status {
        status.format_status()
    } else {
        String::new()
    };

    let element_info = if let Some(node) = tab.current_node() {
        let role = node.role_str();
        let name = node.name_str();
        let max_len = if role.eq_ignore_ascii_case("link") { 60 } else { 30 };
        format!("{}: {}", role, truncate(name, max_len))
    } else {
        String::new()
    };

    let left = format!(" {} | {}", position, state.status_message);
    let center = if !media_info.is_empty() {
        format!(" {} ", media_info)
    } else {
        String::new()
    };
    let right = format!("{} ", element_info);

    let padding = width.saturating_sub(left.len() + center.len() + right.len());
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    format!(
        "\x1b[48;5;240m\x1b[37m{}{}\x1b[33m{}\x1b[37m{}{}\x1b[0m",
        left, " ".repeat(left_pad), center, " ".repeat(right_pad), right
    )
}

/// Build content lines (returns lines and cursor screen line)
fn build_content(state: &BrowserState, width: usize, height: usize) -> (Vec<String>, u16) {
    let tab = state.current_tab();
    let mut lines: Vec<String> = Vec::with_capacity(height);
    let mut cursor_screen_line: u16 = 1;
    let mut cursor_found = false;
    let mut screen_line_idx: usize = 0;
    let mut node_idx = tab.scroll_offset;

    while screen_line_idx < height {
        if let Some(node) = tab.get_node(node_idx) {
            let is_current = node_idx == tab.cursor_index;
            let role = node.role_str();

            if is_current && !cursor_found {
                cursor_screen_line = (screen_line_idx + 1) as u16;
                cursor_found = true;
            }

            let (prefix, content) = format_node(node);
            let full_line = format!("{}{}", prefix, content);
            let wrapped = wrap_text(&full_line, width);
            let color_code = role_color_code(role);

            for (wrap_idx, line_part) in wrapped.iter().enumerate() {
                if screen_line_idx >= height {
                    break;
                }

                let indent = if wrap_idx > 0 { "  " } else { "" };
                let indented_line = format!("{}{}", indent, line_part);
                let indented_len = indented_line.chars().count();
                let padding = " ".repeat(width.saturating_sub(indented_len));

                let line = if is_current {
                    format!("\x1b[44m\x1b[37m{}{}\x1b[0m", indented_line, padding)
                } else {
                    format!("{}{}{}\x1b[0m", color_code, indented_line, padding)
                };

                lines.push(line);
                screen_line_idx += 1;
            }

            node_idx += 1;
        } else {
            lines.push(" ".repeat(width));
            screen_line_idx += 1;
        }
    }

    (lines, cursor_screen_line)
}

/// Get ANSI color code for a role
fn role_color_code(role: &str) -> &'static str {
    match role.to_lowercase().as_str() {
        "heading" => "\x1b[33m",      // Yellow
        "link" => "\x1b[36m",          // Cyan
        "button" => "\x1b[32m",        // Green
        "checkbox" | "radiobutton" => "\x1b[35m", // Magenta
        "textbox" | "textarea" | "textfield" | "combobox" => "\x1b[34m", // Blue
        "navigation" | "main" | "banner" | "contentinfo" => "\x1b[90m", // Dark grey
        _ => "\x1b[37m",               // White
    }
}

/// Wrap text to fit within width, breaking at word boundaries when possible
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;

    for word in text.split_whitespace() {
        let word_len = word.chars().count();

        if current_len == 0 {
            // First word on line
            if word_len > width {
                // Word is longer than width, break it
                let mut chars = word.chars().peekable();
                while chars.peek().is_some() {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if !current_line.is_empty() {
                        lines.push(current_line);
                    }
                    current_line = chunk.clone();
                    current_len = chunk.chars().count();
                }
            } else {
                current_line = word.to_string();
                current_len = word_len;
            }
        } else if current_len + 1 + word_len <= width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_len += 1 + word_len;
        } else {
            // Word doesn't fit, start new line
            lines.push(current_line);
            if word_len > width {
                // Word is longer than width, break it
                let mut chars = word.chars().peekable();
                current_line = String::new();
                current_len = 0;
                while chars.peek().is_some() {
                    let chunk: String = chars.by_ref().take(width).collect();
                    if !current_line.is_empty() {
                        lines.push(current_line);
                    }
                    current_line = chunk.clone();
                    current_len = chunk.chars().count();
                }
            } else {
                current_line = word.to_string();
                current_len = word_len;
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn format_node(node: &crate::accessibility::AXNode) -> (String, String) {
    use crate::accessibility::Role;

    let role = node.role_str();
    let name = node.name_str();

    match role.to_lowercase().as_str() {
        "heading" => {
            // Use h1-h6 based on level (default to h2 if level is 0)
            let level = if node.level > 0 && node.level <= 6 {
                node.level
            } else {
                2
            };
            let prefix = format!("h{} ", level);

            // If heading contains an interactive element, show combined format
            if let Some(ref contained) = node.contains_role {
                match contained {
                    Role::Link => (format!("{}[", prefix), format!("{}]", name)),
                    Role::Button => (format!("{}<", prefix), format!("{}>", name)),
                    _ => (prefix, name.to_string()),
                }
            } else {
                (prefix, name.to_string())
            }
        }
        "link" => ("[".to_string(), format!("{}]", name)),
        "button" => ("<".to_string(), format!("{}>", name)),
        "checkbox" => ("[ ] ".to_string(), name.to_string()),
        "radiobutton" => ("( ) ".to_string(), name.to_string()),
        "textbox" | "textarea" | "textfield" => ("[____] ".to_string(), name.to_string()),
        "listitem" => {
            // Use numbered format if we have position info, otherwise use dash
            if let Some(pos) = node.pos_in_set {
                (format!("{}. ", pos), name.to_string())
            } else {
                ("- ".to_string(), name.to_string())
            }
        }
        "image" => ("[IMG: ".to_string(), format!("{}]", name)),
        "table" => ("TABLE: ".to_string(), name.to_string()),
        "navigation" => ("--- ".to_string(), format!("{} ---", name)),
        "main" => ("=== ".to_string(), format!("{} ===", name)),
        "paragraph" | "statictext" => (String::new(), name.to_string()),
        _ => (String::new(), name.to_string()),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else {
        let truncate_at = max_len.saturating_sub(3);
        let truncated: String = s.chars().take(truncate_at).collect();
        format!("{}...", truncated)
    }
}
