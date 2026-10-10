//! The tray's "Speed limit" submenu: which preset each menu id sets, and which item is checked.

/// Menu id, label and limit in KB/s (0 = no limit), in menu order.
pub const PRESETS: [(&str, &str, u64); 5] = [
    ("speed_0", "Unlimited", 0),
    ("speed_512", "512 KB/s", 512),
    ("speed_1024", "1 MB/s", 1024),
    ("speed_2048", "2 MB/s", 2048),
    ("speed_5120", "5 MB/s", 5120),
];

/// The menu id of "Custom…", which opens the Settings page at the Speed section.
pub const CUSTOM_ID: &str = "speed_custom";

/// The limit a preset menu id sets, or None for any other id.
pub fn preset_kbps(id: &str) -> Option<u64> {
    PRESETS.iter().find(|p| p.0 == id).map(|p| p.2)
}

/// The id of the item to check for the limit in force: its preset, or Custom for any other value.
pub fn checked_id(kbps: u64) -> &'static str {
    PRESETS
        .iter()
        .find(|p| p.2 == kbps)
        .map_or(CUSTOM_ID, |p| p.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_map_to_their_limit() {
        assert_eq!(preset_kbps("speed_0"), Some(0));
        assert_eq!(preset_kbps("speed_512"), Some(512));
        assert_eq!(preset_kbps("speed_1024"), Some(1024));
        assert_eq!(preset_kbps("speed_2048"), Some(2048));
        assert_eq!(preset_kbps("speed_5120"), Some(5120));
    }

    #[test]
    fn other_ids_set_nothing() {
        assert_eq!(preset_kbps(CUSTOM_ID), None);
        assert_eq!(preset_kbps("show"), None);
        assert_eq!(preset_kbps(""), None);
    }

    #[test]
    fn the_matching_preset_is_checked() {
        assert_eq!(checked_id(0), "speed_0");
        assert_eq!(checked_id(512), "speed_512");
        assert_eq!(checked_id(5120), "speed_5120");
    }

    #[test]
    fn any_other_limit_checks_custom() {
        assert_eq!(checked_id(1), CUSTOM_ID);
        assert_eq!(checked_id(1500), CUSTOM_ID);
        assert_eq!(checked_id(10_000_000), CUSTOM_ID);
    }

    #[test]
    fn every_preset_round_trips() {
        for (id, _, kbps) in PRESETS {
            assert_eq!(preset_kbps(id), Some(kbps));
            assert_eq!(checked_id(kbps), id);
        }
    }
}
