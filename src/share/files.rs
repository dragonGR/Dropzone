// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::token::FileId;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Metadata and storage handle for a file shared within a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedFile {
    id: FileId,
    name: String,
    path: PathBuf,
    size_bytes: u64,
}

impl SharedFile {
    /// Creates a new `SharedFile` from an existing local filesystem path.
    pub fn from_path(path: PathBuf) -> io::Result<Self> {
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Shared target must be a regular file",
            ));
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("shared_file")
            .to_string();

        Ok(Self {
            id: FileId::new_random(),
            name,
            path,
            size_bytes: metadata.len(),
        })
    }

    /// Constructs a `SharedFile` explicitly (useful for testing or portal-provided files).
    pub fn new(id: FileId, name: String, path: PathBuf, size_bytes: u64) -> Self {
        Self {
            id,
            name,
            path,
            size_bytes,
        }
    }

    pub fn id(&self) -> &FileId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Formats the file size in human-readable units (matching GNOME style).
    pub fn formatted_size(&self) -> String {
        format_file_size(self.size_bytes)
    }
}

/// Formats a byte count into a human-readable string with units.
pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(1024 * 1024 * 1024 * 2), "2.0 GB");
    }

    #[test]
    fn test_shared_file_from_temp_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("dropzone_test_file.txt");
        std::fs::write(&file_path, b"hello dropzone").expect("write temp file");

        let shared = SharedFile::from_path(file_path.clone()).expect("create SharedFile");
        assert_eq!(shared.name(), "dropzone_test_file.txt");
        assert_eq!(shared.size_bytes(), 14);
        assert_eq!(shared.formatted_size(), "14 B");
        assert_eq!(shared.path(), file_path.as_path());

        let _ = std::fs::remove_file(file_path);
    }
}
