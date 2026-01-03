//! Input handling utilities

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a KeyEvent to a string representation for config matching
pub fn key_to_string(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        // Only add shift for non-character keys
        match key.code {
            KeyCode::Char(_) => {}
            _ => parts.push("shift"),
        }
    }

    let key_str = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::F(n) => format!("f{}", n),
        _ => return String::new(),
    };

    parts.push(&key_str);
    parts.join("+")
}

/// Parse a string representation to check if it matches a key
pub fn string_to_key(s: &str) -> Option<(KeyModifiers, KeyCode)> {
    let parts: Vec<&str> = s.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    let mut key_part = "";

    for part in &parts {
        match *part {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => key_part = part,
        }
    }

    let code = match key_part {
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "esc" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        s if s.len() == 1 => KeyCode::Char(s.chars().next()?),
        s if s.starts_with('f') => {
            let n: u8 = s[1..].parse().ok()?;
            KeyCode::F(n)
        }
        _ => return None,
    };

    Some((modifiers, code))
}
