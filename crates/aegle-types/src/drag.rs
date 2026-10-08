//! Data moved by drag and drop, shared by the platforms and the UI engine.

use std::{path::PathBuf, string::String, vec::Vec};

/// What a drag carries: UTF-8 text or local files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragData {
    /// Plain text.
    Text(String),
    /// Local files and directories.
    Files(Vec<PathBuf>),
}

#[cfg(unix)]
mod uri {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::{Path, PathBuf},
        string::String,
        vec::Vec,
    };

    /// The local path of a `file://` URI with an empty or `localhost` host,
    /// percent-decoded; `None` for other URIs.
    pub fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
        let rest = uri.strip_prefix("file://")?;
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        if !rest.starts_with('/') {
            return None;
        }
        let mut bytes = Vec::with_capacity(rest.len());
        let mut input = rest.bytes();
        while let Some(byte) = input.next() {
            if byte == b'%' {
                let hex = [input.next()?, input.next()?];
                bytes.push(u8::from_str_radix(core::str::from_utf8(&hex).ok()?, 16).ok()?);
            } else {
                bytes.push(byte);
            }
        }
        Some(OsString::from_vec(bytes).into())
    }

    /// A `file://` URI for an absolute `path`, percent-encoding every byte
    /// outside the URI's unreserved set and `/`.
    pub fn file_uri(path: &Path) -> String {
        let mut uri = String::from("file://");
        for &byte in path.as_os_str().as_bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
                uri.push(byte as char);
            } else {
                uri.push('%');
                uri.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
                uri.push(char::from_digit(u32::from(byte & 15), 16).unwrap());
            }
        }
        uri
    }
}

#[cfg(unix)]
pub use uri::{file_uri, path_from_file_uri};
