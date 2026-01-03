//! Text output writer for accessible content.

use std::cell::RefCell;
use std::io::{self, Write};

/// Writes accessible content to any writer
pub struct AccessibleWriter<'a, W: Write> {
    writer: RefCell<&'a mut W>,
}

impl<'a, W: Write> AccessibleWriter<'a, W> {
    pub fn new(writer: &'a mut W) -> Self {
        Self {
            writer: RefCell::new(writer),
        }
    }

    /// Write a heading (Lynx-style)
    pub fn write_heading(&self, level: u8, text: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        let underline = match level {
            1 => "=",
            2 => "-",
            _ => ".",
        };
        writeln!(w, "\n{}", text)?;
        writeln!(w, "{}", underline.repeat(text.len().min(40)))?;
        w.flush()
    }

    /// Write a link
    pub fn write_link(&self, text: &str, _url: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w, "[{}]", text)?;
        w.flush()
    }

    /// Write a button
    pub fn write_button(&self, text: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w, "<{}>", text)?;
        w.flush()
    }

    /// Write plain text
    pub fn write_text(&self, text: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        write!(w, "{} ", text)?;
        w.flush()
    }

    /// Write a newline
    pub fn write_newline(&self) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w)?;
        w.flush()
    }

    /// Write a list item
    pub fn write_list_item(&self, text: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w, "  * {}", text)?;
        w.flush()
    }

    /// Write an image description
    pub fn write_image(&self, alt: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w, "[Image: {}]", alt)?;
        w.flush()
    }

    /// Write a landmark/region
    pub fn write_landmark(&self, name: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        writeln!(w, "\n--- {} ---", name)?;
        w.flush()
    }

    /// Write raw text
    pub fn write_raw(&self, text: &str) -> io::Result<()> {
        let mut w = self.writer.borrow_mut();
        write!(w, "{}", text)?;
        w.flush()
    }
}
