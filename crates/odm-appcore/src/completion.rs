//! "When a download finishes": open its folder or the file itself.
use crate::settings::OnComplete;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAction {
    /// Show the file in its folder.
    OpenFolder,
    OpenFile,
}

/// What to open once `filename` has finished. A file that runs code when opened
/// (a program, a script) never opens by itself: its folder opens instead.
pub fn completion_action(on_complete: OnComplete, filename: &str) -> Option<CompletionAction> {
    match on_complete {
        OnComplete::Nothing => None,
        OnComplete::OpenFolder => Some(CompletionAction::OpenFolder),
        OnComplete::OpenFile if odm_core::filename::is_risky(filename) => {
            Some(CompletionAction::OpenFolder)
        }
        OnComplete::OpenFile => Some(CompletionAction::OpenFile),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_what_the_setting_says() {
        assert_eq!(completion_action(OnComplete::Nothing, "a.pdf"), None);
        assert_eq!(
            completion_action(OnComplete::OpenFolder, "a.pdf"),
            Some(CompletionAction::OpenFolder)
        );
        assert_eq!(
            completion_action(OnComplete::OpenFile, "a.pdf"),
            Some(CompletionAction::OpenFile)
        );
    }

    #[test]
    fn risky_files_never_open_by_themselves() {
        for name in ["setup.exe", "Install.MSI", "run.ps1", "x.bat"] {
            assert_eq!(
                completion_action(OnComplete::OpenFile, name),
                Some(CompletionAction::OpenFolder),
                "{name}"
            );
        }
        assert_eq!(completion_action(OnComplete::Nothing, "setup.exe"), None);
    }
}
