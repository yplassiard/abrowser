#![allow(unused)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::try_block;

const LOG_FILE: &str = "/tmp/abrowser.log";

macro_rules! debug {
    ($($args:expr),+) => {
        crate::utils::log::write(
            "DEBUG",
            file!(),
            line!(),
            &format!($($args),*)
        )
    };
}
macro_rules! warning {
    ($($args:expr),+) => {
        crate::utils::log::write(
            "WARNING",
            file!(),
            line!(),
            &format!($($args),*)
        )
    };
}
macro_rules! error {
    ($($args:expr),+) => {
        crate::utils::log::write(
            "ERROR",
            file!(),
            line!(),
            &format!($($args),*)
        )
    };
}

pub(crate) use debug;
pub(crate) use error;
pub(crate) use warning;

pub fn write(level: &str, file: &str, line: u32, message: &str) {
    let file_name = try_block!(Path::new(file).file_name()?.to_str())
        .flatten()
        .unwrap_or("unknown");
    let log_line = format!("[{level}:{file_name}:{line}] {message}\n");
    let _ = write_to_file(&log_line);
}

/// Write a raw message to the log file
pub fn log(message: &str) {
    let _ = write_to_file(&format!("{message}\n"));
}

fn write_to_file(message: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;
    file.write_all(message.as_bytes())
}
