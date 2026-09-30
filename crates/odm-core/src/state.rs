use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    Queued,
    Probing,
    Downloading,
    Paused,
    Merging,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn as_str(&self) -> &'static str {
        use DownloadState::*;
        match self {
            Queued => "queued",
            Probing => "probing",
            Downloading => "downloading",
            Paused => "paused",
            Merging => "merging",
            Completed => "completed",
            Failed => "failed",
            Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        use DownloadState::*;
        Some(match s {
            "queued" => Queued,
            "probing" => Probing,
            "downloading" => Downloading,
            "paused" => Paused,
            "merging" => Merging,
            "completed" => Completed,
            "failed" => Failed,
            "cancelled" => Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Probing | Self::Downloading | Self::Merging)
    }

    pub fn can_transition(self, to: Self) -> bool {
        use DownloadState::*;
        matches!(
            (self, to),
            (Queued, Probing | Paused | Cancelled)
                | (Probing, Downloading | Paused | Failed | Cancelled | Queued)
                | (
                    Downloading,
                    Merging | Completed | Paused | Failed | Cancelled | Queued
                )
                | (Merging, Completed | Failed | Cancelled)
                | (Paused, Queued | Cancelled)
                | (Failed, Queued | Cancelled)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadState::*;
    use super::*;

    #[test]
    fn round_trips_strings() {
        for s in [
            Queued,
            Probing,
            Downloading,
            Paused,
            Merging,
            Completed,
            Failed,
            Cancelled,
        ] {
            assert_eq!(DownloadState::parse(s.as_str()), Some(s));
        }
        assert_eq!(DownloadState::parse("error"), None);
    }

    #[test]
    fn allowed_transitions() {
        assert!(Queued.can_transition(Probing));
        assert!(Probing.can_transition(Downloading));
        assert!(Downloading.can_transition(Paused));
        assert!(Paused.can_transition(Queued));
        assert!(Downloading.can_transition(Completed));
        assert!(Downloading.can_transition(Merging));
        assert!(Merging.can_transition(Completed));
        assert!(Failed.can_transition(Queued)); // manual retry
        assert!(Downloading.can_transition(Queued)); // reset after crash
    }

    #[test]
    fn forbidden_transitions() {
        assert!(!Completed.can_transition(Downloading));
        assert!(!Cancelled.can_transition(Queued));
        assert!(!Queued.can_transition(Completed));
        assert!(!Paused.can_transition(Downloading)); // must go through Queued
    }

    #[test]
    fn terminal_and_active() {
        assert!(Completed.is_terminal() && Cancelled.is_terminal() && Failed.is_terminal());
        assert!(!Paused.is_terminal());
        assert!(Probing.is_active() && Downloading.is_active() && Merging.is_active());
        assert!(!Queued.is_active());
    }
}
