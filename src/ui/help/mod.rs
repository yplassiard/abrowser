//! Help screen for abrowser
//!
//! Provides HTML/JS interface for:
//! - General usage instructions
//! - Auto-generated keyboard shortcuts from config
//! - Instructions on how to customize shortcuts

use crate::shell::Config;

/// Categorize shortcuts into logical groups for display
struct ShortcutCategory {
    name: &'static str,
    description: &'static str,
    shortcuts: Vec<(String, String, String)>, // (key, action, description)
}

/// Get human-readable description for an action
fn action_description(action: &str) -> String {
    let desc = match action {
        // Cursor movement
        "cursor_up" => "Move cursor up",
        "cursor_down" => "Move cursor down",
        "line_start" => "Go to beginning of line",
        "line_end" => "Go to end of line",
        "page_up" => "Page up",
        "page_down" => "Page down",
        "doc_start" => "Go to beginning of document",
        "doc_end" => "Go to end of document",

        // Element navigation
        "next_heading" => "Next heading",
        "prev_heading" => "Previous heading",
        "next_link" => "Next link",
        "prev_link" => "Previous link",
        "next_button" => "Next button",
        "prev_button" => "Previous button",
        "next_edit" => "Next text field",
        "prev_edit" => "Previous text field",
        "next_checkbox" => "Next checkbox",
        "prev_checkbox" => "Previous checkbox",
        "next_radio" => "Next radio button",
        "prev_radio" => "Previous radio button",
        "next_list" => "Next list",
        "prev_list" => "Previous list",
        "next_table" => "Next table",
        "prev_table" => "Previous table",
        "next_visited" => "Next visited link",
        "prev_visited" => "Previous visited link",
        "next_landmark" => "Next landmark",
        "prev_landmark" => "Previous landmark",
        "next_image" => "Next image",
        "prev_image" => "Previous image",
        "describe_image" => "Describe image with AI",

        // Actions
        "activate" => "Activate/click element",
        "toggle" => "Toggle element/play-pause media",

        // Browser controls
        "address_bar" => "Open address bar",
        "new_tab" => "New tab",
        "close_tab" => "Close current tab",
        "open_file" => "Open local file",
        "save_page" => "Save page",
        "print" => "Print page",
        "downloads" => "Show downloads",
        "toggle_viewport" => "Toggle mobile/desktop viewport",
        "quit" => "Quit abrowser",
        "toggle_mode" => "Toggle focus mode (pass keys to element)",

        // Tab switching
        "switch_tab_0" => "Switch to tab 1",
        "switch_tab_1" => "Switch to tab 2",
        "switch_tab_2" => "Switch to tab 3",
        "switch_tab_3" => "Switch to tab 4",
        "switch_tab_4" => "Switch to tab 5",
        "switch_tab_5" => "Switch to tab 6",
        "switch_tab_6" => "Switch to tab 7",
        "switch_tab_7" => "Switch to tab 8",
        "switch_tab_8" => "Switch to tab 9",
        "switch_tab_9" => "Switch to tab 10",

        // Unknown actions: format nicely (e.g., "my_custom_action" -> "My custom action")
        _ => {
            return action
                .replace('_', " ")
                .split_whitespace()
                .enumerate()
                .map(|(i, word)| {
                    if i == 0 {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                            None => String::new(),
                        }
                    } else {
                        word.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
    };
    desc.to_string()
}

/// Format a key binding for display (e.g., "ctrl+l" -> "Ctrl+L")
fn format_key(key: &str) -> String {
    key.split('+')
        .map(|part| {
            match part.to_lowercase().as_str() {
                "ctrl" => "Ctrl",
                "alt" => "Alt",
                "shift" => "Shift",
                "space" => "Space",
                "enter" => "Enter",
                "tab" => "Tab",
                "esc" => "Esc",
                "home" => "Home",
                "end" => "End",
                "pageup" => "PageUp",
                "pagedown" => "PageDown",
                "up" => "Up",
                "down" => "Down",
                "left" => "Left",
                "right" => "Right",
                other => return other.to_uppercase(),
            }
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Categorize a shortcut based on its action
fn categorize_action(action: &str) -> &'static str {
    match action {
        // Cursor movement
        "cursor_up" | "cursor_down" | "line_start" | "line_end" | "page_up" | "page_down"
        | "doc_start" | "doc_end" => "Navigation",

        // Element navigation
        "next_heading" | "prev_heading" | "next_link" | "prev_link" | "next_button"
        | "prev_button" | "next_edit" | "prev_edit" | "next_checkbox" | "prev_checkbox"
        | "next_radio" | "prev_radio" | "next_list" | "prev_list" | "next_table" | "prev_table"
        | "next_visited" | "prev_visited" | "next_landmark" | "prev_landmark" | "next_image"
        | "prev_image" => "Quick Navigation",

        // AI
        "describe_image" => "AI Features",

        // Actions
        "activate" | "toggle" => "Actions",

        // Browser controls
        "address_bar" | "new_tab" | "close_tab" | "open_file" | "save_page" | "print"
        | "downloads" | "toggle_viewport" | "quit" | "toggle_mode" => "Browser Controls",

        // Tab switching
        action if action.starts_with("switch_tab_") => "Tab Switching",

        _ => "Other",
    }
}

/// Generate shortcuts from the config's KeyBindings
fn generate_shortcuts_from_config(config: &Config) -> Vec<ShortcutCategory> {
    use std::collections::HashMap;

    let bindings = &config.navigation_keys.bindings;

    // Group shortcuts by category
    let mut categories_map: HashMap<&str, Vec<(String, String, String)>> = HashMap::new();

    for (key, action) in bindings {
        let category = categorize_action(action);
        let formatted_key = format_key(key);
        let description = action_description(action);

        categories_map
            .entry(category)
            .or_default()
            .push((formatted_key, action.clone(), description));
    }

    // Add hardcoded shortcuts that aren't in the config
    let hardcoded = vec![
        ("Navigation", vec![
            ("Left", "cursor_left", "Select previous clickable element within line"),
            ("Right", "cursor_right", "Select next clickable element within line"),
            ("Alt+Left", "back", "Go back in history"),
            ("Alt+Right", "forward", "Go forward in history"),
            ("F4", "refresh_tree", "Refresh accessibility tree"),
            ("F5", "refresh_page", "Refresh page"),
        ]),
        ("Browser Controls", vec![
            ("F1", "help", "Show this help screen"),
            ("F2", "options", "Open options"),
            ("Ctrl+R", "refresh", "Refresh page"),
            ("Ctrl+H", "history", "Show history menu"),
        ]),
        ("Search", vec![
            ("/", "search_forward", "Search forward"),
            ("?", "search_backward", "Search backward"),
            ("s", "search_next", "Find next match"),
            ("S", "search_prev", "Find previous match"),
        ]),
        ("Tab Navigation", vec![
            ("<", "prev_tab", "Previous tab"),
            (">", "next_tab", "Next tab"),
            ("Alt+0-9", "switch_tab", "Switch to tab by number"),
        ]),
        ("Media Controls", vec![
            ("Space", "play_pause", "Play/pause media"),
            ("p", "play_pause", "Play/pause media"),
            ("m", "mute", "Toggle mute"),
            ("j", "rewind_10s", "Rewind 10 seconds"),
            (";", "forward_10s", "Forward 10 seconds"),
            (",", "rewind_5s", "Rewind 5 seconds"),
            (".", "forward_5s", "Forward 5 seconds"),
            ("[", "speed_down", "Decrease playback speed"),
            ("]", "speed_up", "Increase playback speed"),
            ("0-9", "seek_percent", "Seek to percentage"),
            ("+", "volume_up", "Increase volume"),
            ("-", "volume_down", "Decrease volume"),
        ]),
        ("Text Editing", vec![
            ("Tab", "next_focus", "Move to next focusable element"),
            ("Shift+Tab", "prev_focus", "Move to previous focusable element"),
            ("Ctrl+U", "clear_line", "Clear input line"),
            ("Ctrl+W", "delete_word", "Delete word before cursor"),
            ("Ctrl+K", "delete_to_end", "Delete to end of line"),
            ("Ctrl+A", "line_start", "Move to line start"),
            ("Ctrl+E", "line_end", "Move to line end"),
        ]),
    ];

    for (cat_name, shortcuts) in hardcoded {
        for (key, action, desc) in shortcuts {
            // Only add if not already present from config
            let entry = categories_map.entry(cat_name).or_default();
            if !entry.iter().any(|(k, _, _)| k == key) {
                entry.push((key.to_string(), action.to_string(), desc.to_string()));
            }
        }
    }

    // Define category order and descriptions
    let category_info: Vec<(&str, &str)> = vec![
        ("Navigation", "Basic cursor movement and page navigation"),
        (
            "Quick Navigation",
            "Jump directly to specific element types",
        ),
        (
            "Browser Controls",
            "Core browser functions like tabs, address bar, etc.",
        ),
        ("Search", "Text search within the page"),
        ("Tab Navigation", "Switch between open tabs"),
        ("Actions", "Interact with elements"),
        ("AI Features", "AI-powered accessibility features"),
        ("Media Controls", "Control video and audio playback"),
        ("Text Editing", "Edit text in form fields"),
        ("Tab Switching", "Switch to specific tabs by number"),
        ("Other", "Additional shortcuts"),
    ];

    // Build final categories list in order
    let mut categories = Vec::new();
    for (name, desc) in category_info {
        if let Some(mut shortcuts) = categories_map.remove(name) {
            // Sort shortcuts by key for consistent display
            shortcuts.sort_by(|a, b| a.0.cmp(&b.0));

            categories.push(ShortcutCategory {
                name,
                description: desc,
                shortcuts,
            });
        }
    }

    categories
}

/// Generate the help HTML page
pub fn generate_help_html(config: &Config) -> String {
    let categories = generate_shortcuts_from_config(config);

    // Generate shortcuts HTML
    let shortcuts_html: String = categories
        .iter()
        .filter(|c| !c.shortcuts.is_empty())
        .map(|category| {
            let shortcuts_rows: String = category
                .shortcuts
                .iter()
                .map(|(key, _action, desc)| {
                    format!(
                        "<li>{}: {}</li>",
                        html_escape(key),
                        html_escape(desc)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"
                <div class="category">
                    <h3>{}</h3>
                    <p class="category-desc">{}</p>
                    <ul>
                        {}
                    </ul>
                </div>
                "#,
                category.name, category.description, shortcuts_rows
            )
        })
        .collect();

    // Get config file path for display
    let config_path = dirs::config_dir()
        .map(|p| p.join("abrowser").join("config.toml"))
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/abrowser/config.toml".to_string());

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>abrowser Help</title>
    <style>
        :root {{
            --bg-primary: #1a1a2e;
            --bg-secondary: #16213e;
            --bg-card: #0f3460;
            --text-primary: #eee;
            --text-secondary: #aaa;
            --accent: #e94560;
            --accent-hover: #ff6b6b;
            --success: #4ecca3;
            --border: #444;
        }}

        * {{ box-sizing: border-box; margin: 0; padding: 0; }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
            max-width: 900px;
            margin: 0 auto;
        }}

        h1 {{ color: var(--accent); margin-bottom: 0.5rem; }}
        h2 {{ color: var(--text-primary); margin: 1.5rem 0 1rem; border-bottom: 1px solid var(--border); padding-bottom: 0.5rem; }}
        h3 {{ color: var(--accent); margin-bottom: 0.5rem; font-size: 1.1rem; }}

        .subtitle {{ color: var(--text-secondary); margin-bottom: 1.5rem; }}

        .section {{
            background: var(--bg-secondary);
            border-radius: 8px;
            padding: 1.5rem;
            margin-bottom: 1.5rem;
        }}

        .section p {{ margin-bottom: 0.75rem; }}
        .section ul {{ margin-left: 1.5rem; margin-bottom: 0.75rem; }}
        .section li {{ margin-bottom: 0.25rem; }}

        .category {{
            background: var(--bg-card);
            border-radius: 6px;
            padding: 1rem;
            margin-bottom: 1rem;
        }}

        .category-desc {{
            color: var(--text-secondary);
            font-size: 0.9rem;
            margin-bottom: 0.75rem;
        }}

        .category ul {{
            margin-left: 1.5rem;
        }}

        .category li {{
            margin-bottom: 0.25rem;
        }}

        footer {{
            margin-top: 2rem;
            text-align: center;
            color: var(--text-secondary);
            font-size: 0.9rem;
        }}

        footer a {{ color: var(--accent); text-decoration: none; }}
        footer a:hover {{ text-decoration: underline; }}

        .tip {{
            background: var(--bg-card);
            border-left: 4px solid var(--success);
            padding: 0.75rem 1rem;
            margin: 1rem 0;
            border-radius: 0 4px 4px 0;
        }}
    </style>
</head>
<body>
    <h1>abrowser Help</h1>
    <p class="subtitle">Accessible Terminal Browser - Keyboard Reference</p>

    <div class="section">
        <h2>Getting Started</h2>
        <p>abrowser is a fully keyboard-accessible web browser that runs in your terminal. It reads web pages through the accessibility tree, making web content accessible to screen reader users.</p>
        <p class="tip">Tip: Press Esc to close this help screen and return to browsing.</p>

        <h3>Basic Usage</h3>
        <ul>
            <li>Use Ctrl+L to open the address bar and enter a URL or search term</li>
            <li>Navigate through the page using Up/Down arrows</li>
            <li>Press Enter to activate links and buttons</li>
            <li>Use quick navigation keys (h, k, b, etc.) to jump to specific element types</li>
        </ul>

        <h3>Focus Modes</h3>
        <p>Navigation Mode (default): All keyboard shortcuts are active. Use this mode for browsing and navigating pages.</p>
        <p>Focus Mode: Keys are passed directly to the focused element (e.g., for typing in text fields). Toggle with Ctrl+Space.</p>
    </div>

    <div class="section">
        <h2>Keyboard Shortcuts</h2>
        <p>Customizing Shortcuts: You can customize keyboard shortcuts by editing the config file at {config_path}. Add or modify key bindings under [navigation_keys.bindings], for example: ctrl+n = "new_tab"</p>

        {shortcuts_html}
    </div>

    <div class="section">
        <h2>Quick Navigation Keys</h2>
        <p>Use these single-key shortcuts to jump between elements of a specific type. Lowercase letters jump to the next element of that type (e.g., h for next heading). Uppercase letters jump to the previous element (e.g., H for previous heading).</p>
    </div>

    <div class="section">
        <h2>Element Display Format</h2>
        <ul>
            <li>h1-h6: Headings (e.g., h1 Welcome)</li>
            <li>[Link text]: Clickable links</li>
            <li>&lt; Button text &gt;: Buttons</li>
            <li>(Image: description): Images</li>
            <li>[ ] / [x]: Unchecked / checked checkbox</li>
            <li>( ) / (x): Unchecked / checked radio button</li>
            <li>-- value -- or -- _____ --: Text input field</li>
            <li>- Text: List item</li>
        </ul>
        <p>When an element contains multiple clickable targets (e.g., a list item with a link), use Left/Right arrows to select which to activate, then press Enter.</p>
    </div>

    <footer>
        <p>abrowser - Accessible Terminal Browser</p>
        <p><a href="https://github.com/yplassiard/abrowser">GitHub</a> | Press F2 for Options</p>
    </footer>
</body>
</html>"#,
        config_path = config_path,
        shortcuts_html = shortcuts_html,
    )
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
