//! Terminal rendering for the browser shell

use super::{BrowserState, Config, FocusMode};
use crossterm::{
    cursor::{self, MoveTo},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::io::{self, Write};

/// Render the browser state to the terminal
pub fn render(state: &BrowserState, _config: &Config) -> io::Result<()> {
    let mut stdout = io::stdout();
    let (width, height) = terminal::size()?;

    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;

    // Tab bar (line 0)
    render_tab_bar(&mut stdout, state, width)?;

    // URL bar (line 1)
    render_url_bar(&mut stdout, state, width)?;

    // Content area (lines 2 to height-2)
    let content_height = height.saturating_sub(4) as usize;
    let cursor_line = render_content(&mut stdout, state, width as usize, content_height)?;

    // Status bar (last line)
    execute!(stdout, MoveTo(0, height - 1))?;
    render_status_bar(&mut stdout, state, width)?;

    // Input prompt if active
    if let Some(ref prompt) = state.input_prompt {
        execute!(stdout, MoveTo(0, height - 2))?;
        execute!(
            stdout,
            SetBackgroundColor(Color::DarkBlue),
            SetForegroundColor(Color::White),
            Print(format!("{}{}", prompt, state.input_value)),
            Print(" ".repeat((width as usize).saturating_sub(prompt.len() + state.input_value.chars().count()))),
            ResetColor
        )?;
        // Position cursor at input_cursor position (count chars, not bytes)
        let cursor_char_pos = state.input_value[..state.input_cursor].chars().count();
        execute!(
            stdout,
            cursor::Show,
            MoveTo((prompt.len() + cursor_char_pos) as u16, height - 2)
        )?;
    } else {
        // Position cursor at beginning of current element (column 0, always)
        // Ensure cursor_line is within visible content area
        let safe_cursor_line = cursor_line.max(2).min(height - 2);
        execute!(
            stdout,
            cursor::Show,
            MoveTo(0, safe_cursor_line)
        )?;
    }

    stdout.flush()
}

fn render_tab_bar(stdout: &mut io::Stdout, state: &BrowserState, width: u16) -> io::Result<()> {
    execute!(
        stdout,
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White)
    )?;

    let mut x = 0;
    for (i, tab) in state.tabs.iter().enumerate() {
        let title = if tab.title.is_empty() {
            if tab.url.is_empty() {
                "New Tab"
            } else {
                &tab.url
            }
        } else {
            &tab.title
        };

        // Truncate title
        let title: String = title.chars().take(20).collect();
        let tab_text = format!(" {} {} ", i, title);

        if i == state.current_tab_index {
            execute!(
                stdout,
                SetBackgroundColor(Color::Blue),
                SetForegroundColor(Color::White),
                Print(&tab_text),
                SetBackgroundColor(Color::DarkGrey)
            )?;
        } else {
            execute!(stdout, Print(&tab_text))?;
        }

        x += tab_text.len() as u16;
        if x >= width {
            break;
        }
    }

    // Fill rest of line
    let remaining = (width as usize).saturating_sub(x as usize);
    execute!(stdout, Print(" ".repeat(remaining)), ResetColor)?;

    Ok(())
}

fn render_url_bar(stdout: &mut io::Stdout, state: &BrowserState, width: u16) -> io::Result<()> {
    execute!(stdout, MoveTo(0, 1))?;

    let tab = state.current_tab();
    let mode_indicator = match state.focus_mode {
        FocusMode::Navigation => "[NAV]",
        FocusMode::Focus => "[FOC]",
    };

    let url = if tab.url.is_empty() {
        "about:blank"
    } else {
        &tab.url
    };

    // Truncate URL to fit
    let max_url_len = (width as usize).saturating_sub(mode_indicator.len() + 3);
    let display_url: String = if url.len() > max_url_len {
        format!("{}...", &url[..max_url_len.saturating_sub(3)])
    } else {
        url.to_string()
    };

    execute!(
        stdout,
        SetBackgroundColor(Color::Black),
        SetForegroundColor(Color::Cyan),
        Print(mode_indicator),
        Print(" "),
        SetForegroundColor(Color::White),
        Print(&display_url),
        Print(" ".repeat((width as usize).saturating_sub(mode_indicator.len() + 1 + display_url.len()))),
        ResetColor
    )?;

    Ok(())
}

fn render_content(
    stdout: &mut io::Stdout,
    state: &BrowserState,
    width: usize,
    height: usize,
) -> io::Result<u16> {
    let tab = state.current_tab();
    let mut cursor_screen_line: u16 = 2; // Default to first content line
    let mut cursor_found = false;
    let mut screen_line_idx: usize = 0;

    // Build wrapped lines for visible content
    let mut node_idx = tab.scroll_offset;

    while screen_line_idx < height {
        let screen_line = (screen_line_idx + 2) as u16;

        if let Some(node) = tab.get_node(node_idx) {
            let is_current = node_idx == tab.cursor_index;
            let role = node.role_str();
            let name = node.name_str();

            // Track cursor position (first line of current element)
            if is_current && !cursor_found {
                cursor_screen_line = screen_line;
                cursor_found = true;
            }

            // Format based on role
            let (prefix, content) = format_node(role, name);
            let full_line = format!("{}{}", prefix, content);

            // Wrap the line
            let wrapped = wrap_text(&full_line, width);
            let color = role_color(role);

            for (wrap_idx, line_part) in wrapped.iter().enumerate() {
                if screen_line_idx >= height {
                    break;
                }

                let current_screen_line = (screen_line_idx + 2) as u16;
                execute!(stdout, MoveTo(0, current_screen_line))?;

                let indent = if wrap_idx > 0 { "  " } else { "" };
                let indented_line = format!("{}{}", indent, line_part);
                let indented_len = indented_line.chars().count();

                if is_current {
                    execute!(
                        stdout,
                        SetBackgroundColor(Color::Blue),
                        SetForegroundColor(Color::White),
                        Print(&indented_line),
                        Print(" ".repeat(width.saturating_sub(indented_len))),
                        ResetColor
                    )?;
                } else {
                    execute!(
                        stdout,
                        SetForegroundColor(color),
                        Print(&indented_line),
                        Print(" ".repeat(width.saturating_sub(indented_len))),
                        ResetColor
                    )?;
                }

                screen_line_idx += 1;
            }

            node_idx += 1;
        } else {
            // No more nodes, fill remaining lines
            execute!(stdout, MoveTo(0, screen_line))?;
            execute!(stdout, Print(" ".repeat(width)))?;
            screen_line_idx += 1;
        }
    }

    Ok(cursor_screen_line)
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

fn format_node(role: &str, name: &str) -> (&'static str, String) {
    match role.to_lowercase().as_str() {
        "heading" => ("# ", name.to_string()),
        "link" => ("[", format!("{}]", name)),
        "button" => ("<", format!("{}>", name)),
        "checkbox" => ("[ ] ", name.to_string()),
        "radiobutton" => ("( ) ", name.to_string()),
        "textbox" | "textarea" | "textfield" => ("[____] ", name.to_string()),
        "listitem" => ("  * ", name.to_string()),
        "image" => ("[IMG: ", format!("{}]", name)),
        "table" => ("TABLE: ", name.to_string()),
        "navigation" => ("--- ", format!("{} ---", name)),
        "main" => ("=== ", format!("{} ===", name)),
        "paragraph" | "statictext" => ("", name.to_string()),
        _ => ("", name.to_string()),
    }
}

fn role_color(role: &str) -> Color {
    match role.to_lowercase().as_str() {
        "heading" => Color::Yellow,
        "link" => Color::Cyan,
        "button" => Color::Green,
        "checkbox" | "radiobutton" => Color::Magenta,
        "textbox" | "textarea" | "textfield" => Color::Blue,
        "navigation" | "main" | "banner" | "contentinfo" => Color::DarkGrey,
        _ => Color::White,
    }
}

fn render_status_bar(stdout: &mut io::Stdout, state: &BrowserState, width: u16) -> io::Result<()> {
    let tab = state.current_tab();

    // Position indicator
    let position = if tab.node_count() > 0 {
        format!("{}/{}", tab.cursor_index + 1, tab.node_count())
    } else {
        "0/0".to_string()
    };

    // Media status (if playing)
    let media_info = if let Some(ref status) = state.media_status {
        status.format_status()
    } else {
        String::new()
    };

    // Current element info (show full text for links)
    let element_info = if let Some(node) = tab.current_node() {
        let role = node.role_str();
        let name = node.name_str();
        // Show more text for links since URLs/link text can be important
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

    let padding = (width as usize).saturating_sub(left.len() + center.len() + right.len());
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    execute!(
        stdout,
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White),
        Print(&left),
        Print(" ".repeat(left_pad)),
        SetForegroundColor(Color::Yellow),
        Print(&center),
        SetForegroundColor(Color::White),
        Print(" ".repeat(right_pad)),
        Print(&right),
        ResetColor
    )?;

    Ok(())
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
