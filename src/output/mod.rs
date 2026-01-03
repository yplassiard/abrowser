//! Output rendering for accessible content.
//!
//! Unlike Carbonyl's pixel-based rendering, abrowser outputs
//! linearized semantic text for screen readers and braille displays.

mod writer;

pub use writer::*;

use std::io::{self, Write};

/// Terminal size in characters
#[derive(Clone, Debug)]
pub struct TerminalSize {
    pub cols: usize,
    pub rows: usize,
}

impl TerminalSize {
    pub fn get() -> io::Result<Self> {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) };

        if result == -1 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            cols: size.ws_col as usize,
            rows: size.ws_row as usize,
        })
    }
}

/// Clear the screen and move cursor to top-left
pub fn clear_screen() -> io::Result<()> {
    let mut out = io::stdout();
    write!(out, "\x1b[2J\x1b[H")?;
    out.flush()
}

/// Move cursor to position (1-indexed)
pub fn move_cursor(row: usize, col: usize) -> io::Result<()> {
    let mut out = io::stdout();
    write!(out, "\x1b[{};{}H", row, col)?;
    out.flush()
}
