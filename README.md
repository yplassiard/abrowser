# abrowser

An accessible terminal browser with screen reader and braille display support.

## Overview

abrowser is a Chromium-based browser designed for accessibility. Unlike visual terminal browsers, abrowser focuses on providing semantic access to web content through the accessibility tree (AXTree).

## Features

- **Text mode**: Lynx-style output with numbered links
- **Screen reader mode**: Semantic announcements optimized for screen readers
- **Braille mode**: Fixed-width output for braille displays (40/80 cells)
- **Virtual cursor navigation**: Navigate by element, heading, landmark, link, etc.
- **Full keyboard support**: Tab navigation, arrow keys, shortcuts

## Output Modes

### Text Mode (default)
```
Welcome to Example.com
========================

[1] Home  [2] About  [3] Contact

Main Content
------------

This is a paragraph with a [4]link to somewhere.

  * List item 1
  * List item 2

[Submit]

---
Links: [1] / [2] /about [3] /contact [4] https://example.com/somewhere
```

### Screen Reader Mode
```
heading level 1: Welcome to Example.com
navigation landmark
  link: Home
  link: About
  link: Contact
main landmark
  heading level 2: Main Content
  text: This is a paragraph with a
  link: link to somewhere
  list with 2 items
    list item: List item 1
    list item: List item 2
  button: Submit
```

### Braille Mode (40-cell)
```
h1 Welcome to Example.com
nav [Home] [About] [Contact]
h2 Main Content
This is a paragraph with a lk[link
to somewhere].
* List item 1
* List item 2
btn[Submit]
```

## Usage

```bash
abrowser [OPTIONS] [URL]

OPTIONS:
    -t, --text           Lynx-style text output (default)
    -s, --screen-reader  Screen reader optimized output
    -b, --braille[=N]    Braille display output (N cells, default 40)
    -d, --debug          Enable debug logging
    -h, --help           Show help
    -v, --version        Show version
```

## Navigation

| Key | Action |
|-----|--------|
| Tab / Shift+Tab | Move between links/controls |
| Enter | Activate link/button |
| Arrow keys | Scroll / navigate |
| H / Shift+H | Next/previous heading |
| K / Shift+K | Next/previous link |
| L | Next landmark |
| Backspace | Go back |
| Ctrl+L | Focus address bar |
| Ctrl+C | Exit |

## Building

Requires Chromium source and build tools. See [Building Guide](docs/building.md).

```bash
# Clone and setup
git clone https://github.com/user/abrowser
cd abrowser

# Fetch Chromium (takes a while)
cd chromium
gclient sync

# Apply patches
./scripts/apply-patches.sh

# Build
./scripts/build.sh
```

## Architecture

```
Chromium Blink → AXTree → BrowserAccessibilityManager
                               ↓
                    Rust Accessibility Bridge
                               ↓
                    Virtual Cursor & Linearizer
                               ↓
                    Accessible Text Output
```

## Acknowledgments

- Input handling adapted from [Carbonyl](https://github.com/nicholasday/carbonyl) (MIT License)
- Chromium build infrastructure inspired by Carbonyl

## License

MIT
