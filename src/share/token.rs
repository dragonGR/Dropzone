// SPDX-License-Identifier: GPL-3.0-or-later

use rand::RngCore;
use rand::rngs::OsRng;
use std::fmt;

/// A cryptographically secure, 256-bit capability token that authorizes
/// access to an ephemeral sharing session.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ShareToken(String);

impl ShareToken {
    /// Generates a new 256-bit random capability token from the OS CSPRNG.
    pub fn new_random() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let mut hex = String::with_capacity(64);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", b);
        }
        Self(hex)
    }

    /// Returns the string representation of the token.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates whether an incoming token string conforms to expected format.
    pub fn is_valid_format(candidate: &str) -> bool {
        candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl fmt::Display for ShareToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ShareToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redact secret capability token from debug output to prevent log leakage.
        write!(f, "ShareToken([REDACTED])")
    }
}

/// An opaque, randomly generated file identifier for a file shared within a session.
/// Independent of the file's disk path and name.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FileId(String);

impl FileId {
    /// Generates a new 128-bit random file identifier from the OS CSPRNG.
    pub fn new_random() -> Self {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        let mut hex = String::with_capacity(32);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(hex, "{:02x}", b);
        }
        Self(hex)
    }

    /// Returns the string representation of the file identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates whether an incoming identifier conforms to expected format.
    pub fn is_valid_format(candidate: &str) -> bool {
        candidate.len() == 32 && candidate.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_randomness_and_format() {
        let t1 = ShareToken::new_random();
        let t2 = ShareToken::new_random();

        assert_ne!(t1, t2);
        assert_eq!(t1.as_str().len(), 64);
        assert!(ShareToken::is_valid_format(t1.as_str()));
        assert!(ShareToken::is_valid_format(t2.as_str()));
    }

    #[test]
    fn test_token_debug_redacted() {
        let token = ShareToken::new_random();
        let debug_str = format!("{:?}", token);
        assert!(!debug_str.contains(token.as_str()));
        assert_eq!(debug_str, "ShareToken([REDACTED])");
    }

    #[test]
    fn test_token_invalid_format_rejected() {
        assert!(!ShareToken::is_valid_format(""));
        assert!(!ShareToken::is_valid_format("too_short"));
        let non_hex = "g".repeat(64);
        assert!(!ShareToken::is_valid_format(&non_hex));
        let long = "0".repeat(65);
        assert!(!ShareToken::is_valid_format(&long));
    }

    #[test]
    fn test_file_id_randomness_and_format() {
        let id1 = FileId::new_random();
        let id2 = FileId::new_random();

        assert_ne!(id1, id2);
        assert_eq!(id1.as_str().len(), 32);
        assert!(FileId::is_valid_format(id1.as_str()));
        assert!(FileId::is_valid_format(id2.as_str()));
    }

    #[test]
    fn test_file_id_invalid_format_rejected() {
        assert!(!FileId::is_valid_format(""));
        assert!(!FileId::is_valid_format("1234"));
        let non_hex = "z".repeat(32);
        assert!(!FileId::is_valid_format(&non_hex));
    }
}
