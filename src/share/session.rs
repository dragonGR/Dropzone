// SPDX-License-Identifier: GPL-3.0-or-later

use crate::share::files::SharedFile;
use crate::share::token::ShareToken;

/// Represents an active or stopped sharing session.
#[derive(Debug, Clone)]
pub struct ShareSession {
    token: ShareToken,
    file: SharedFile,
    active: bool,
}

impl ShareSession {
    /// Initializes a new active sharing session for a single file.
    pub fn new(file: SharedFile) -> Self {
        Self {
            token: ShareToken::new_random(),
            file,
            active: true,
        }
    }

    pub fn token(&self) -> &ShareToken {
        &self.token
    }

    pub fn file(&self) -> &SharedFile {
        &self.file
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Stops the sharing session. An inactive session rejects all future requests.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Checks whether the provided capability token matches this active session.
    pub fn is_authorized(&self, token_candidate: &str) -> bool {
        self.active && self.token.as_str() == token_candidate
    }

    /// Retrieves the shared file if authorized by token and matching the file ID.
    pub fn get_authorized_file(
        &self,
        token_candidate: &str,
        file_id_candidate: &str,
    ) -> Option<&SharedFile> {
        if !self.is_authorized(token_candidate) {
            return None;
        }

        if self.file.id().as_str() == file_id_candidate {
            Some(&self.file)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::token::FileId;
    use std::path::PathBuf;

    fn sample_file() -> SharedFile {
        SharedFile::new(
            FileId::new_random(),
            "photo.jpg".to_string(),
            PathBuf::from("/tmp/photo.jpg"),
            1024,
        )
    }

    #[test]
    fn test_session_lifecycle_and_authorization() {
        let file = sample_file();
        let file_id_str = file.id().as_str().to_string();
        let mut session = ShareSession::new(file);

        assert!(session.is_active());

        let valid_token = session.token().as_str().to_string();
        let invalid_token = "0".repeat(64);

        assert!(session.is_authorized(&valid_token));
        assert!(!session.is_authorized(&invalid_token));

        assert!(
            session
                .get_authorized_file(&valid_token, &file_id_str)
                .is_some()
        );
        assert!(
            session
                .get_authorized_file(&valid_token, "wrong_file_id")
                .is_none()
        );
        assert!(
            session
                .get_authorized_file(&invalid_token, &file_id_str)
                .is_none()
        );

        session.stop();
        assert!(!session.is_active());

        assert!(!session.is_authorized(&valid_token));
        assert!(
            session
                .get_authorized_file(&valid_token, &file_id_str)
                .is_none()
        );
    }
}
