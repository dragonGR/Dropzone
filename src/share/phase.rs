// SPDX-License-Identifier: GPL-3.0-or-later

/// Identifies one attempt to start sharing, so that a start which completes after
/// the user stopped or restarted can be recognised as stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartAttempt(u64);

enum Phase<S> {
    Idle,
    Starting(StartAttempt),
    Sharing(S),
}

/// The window's sharing state: idle, starting a share, or sharing.
///
/// `S` is whatever the window keeps alive while sharing.
pub struct SharePhase<S> {
    phase: Phase<S>,
    last_attempt: u64,
}

impl<S> Default for SharePhase<S> {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            last_attempt: 0,
        }
    }
}

impl<S> SharePhase<S> {
    pub fn is_idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    /// Begins starting a share. Returns `None` unless the window is idle.
    pub fn begin_start(&mut self) -> Option<StartAttempt> {
        if !self.is_idle() {
            return None;
        }
        self.last_attempt += 1;
        let attempt = StartAttempt(self.last_attempt);
        self.phase = Phase::Starting(attempt);
        Some(attempt)
    }

    /// Completes `attempt` with the running share. A stale attempt hands the share
    /// back so the caller can shut it down.
    pub fn finish_start(&mut self, attempt: StartAttempt, share: S) -> Result<(), S> {
        match self.phase {
            Phase::Starting(current) if current == attempt => {
                self.phase = Phase::Sharing(share);
                Ok(())
            }
            _ => Err(share),
        }
    }

    /// Abandons `attempt` after it failed. Returns whether it was still current, so
    /// that errors from stale attempts are not shown.
    pub fn fail_start(&mut self, attempt: StartAttempt) -> bool {
        match self.phase {
            Phase::Starting(current) if current == attempt => {
                self.phase = Phase::Idle;
                true
            }
            _ => false,
        }
    }

    /// Returns to idle, handing back the running share if there is one.
    pub fn stop(&mut self) -> Option<S> {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Sharing(share) => Some(share),
            Phase::Idle | Phase::Starting(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_start_is_refused_while_starting() {
        let mut phase = SharePhase::<&str>::default();
        assert!(phase.begin_start().is_some());
        assert!(phase.begin_start().is_none());
        assert!(!phase.is_idle());
    }

    #[test]
    fn test_second_start_is_refused_while_sharing() {
        let mut phase = SharePhase::default();
        let attempt = phase.begin_start().expect("idle");
        assert_eq!(phase.finish_start(attempt, "share"), Ok(()));
        assert!(phase.begin_start().is_none());
    }

    #[test]
    fn test_start_completing_after_stop_is_handed_back() {
        let mut phase = SharePhase::default();
        let attempt = phase.begin_start().expect("idle");
        assert_eq!(phase.stop(), None);
        assert!(phase.is_idle());
        assert_eq!(phase.finish_start(attempt, "late share"), Err("late share"));
        assert!(phase.is_idle());
    }

    #[test]
    fn test_start_from_earlier_attempt_is_handed_back() {
        let mut phase = SharePhase::default();
        let first = phase.begin_start().expect("idle");
        phase.stop();
        let second = phase.begin_start().expect("idle again");
        assert_eq!(phase.finish_start(first, "stale"), Err("stale"));
        assert_eq!(phase.finish_start(second, "current"), Ok(()));
        assert_eq!(phase.stop(), Some("current"));
    }

    #[test]
    fn test_failure_of_stale_attempt_is_not_reported() {
        let mut phase = SharePhase::<&str>::default();
        let first = phase.begin_start().expect("idle");
        phase.stop();
        let second = phase.begin_start().expect("idle again");
        assert!(!phase.fail_start(first));
        assert!(
            !phase.is_idle(),
            "a stale failure must not cancel the current attempt"
        );
        assert!(phase.fail_start(second));
        assert!(phase.is_idle());
    }

    #[test]
    fn test_stop_hands_back_the_share_once() {
        let mut phase = SharePhase::default();
        let attempt = phase.begin_start().expect("idle");
        phase
            .finish_start(attempt, "share")
            .expect("current attempt");
        assert_eq!(phase.stop(), Some("share"));
        assert_eq!(phase.stop(), None);
        assert!(phase.is_idle());
    }
}
