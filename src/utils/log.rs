#![allow(unused)]

use std::path::Path;

use crate::try_block;

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
    eprintln!("[{level}:{file_name}:{line}] {message}");
}
