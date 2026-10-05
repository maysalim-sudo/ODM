//! Browser downloads waiting for the "New download" dialog.
use std::sync::{Mutex, MutexGuard};

/// Ids of downloads added paused that still await the user's choice, oldest first.
#[derive(Default)]
pub struct Pending {
    ids: Mutex<Vec<String>>,
}

impl Pending {
    fn ids(&self) -> MutexGuard<'_, Vec<String>> {
        self.ids.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds `id` at the end; an id already waiting keeps its place.
    pub fn push(&self, id: String) {
        let mut ids = self.ids();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.ids().clone()
    }

    /// True when `id` was waiting.
    pub fn remove(&self, id: &str) -> bool {
        let mut ids = self.ids();
        let before = ids.len();
        ids.retain(|i| i != id);
        ids.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_list_remove_keep_order_without_duplicates() {
        let p = Pending::default();
        p.push("a".into());
        p.push("b".into());
        p.push("a".into());
        assert_eq!(p.list(), vec!["a", "b"]);
        assert!(p.remove("a"));
        assert!(!p.remove("a"));
        assert_eq!(p.list(), vec!["b"]);
    }
}
