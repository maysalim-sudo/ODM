//! What ODM reports as a torrent's progress: never less than before. librqbit can
//! briefly report less (a piece failing its hash check); the bar must not jump back.

#[derive(Debug, Default)]
pub struct Monotonic {
    last: u64,
}

impl Monotonic {
    /// The bytes to report for `done` of `total` selected bytes.
    pub fn report(&mut self, done: u64, total: u64) -> u64 {
        // A smaller selection can bring the total below what was reported.
        self.last = self.last.min(total).max(done.min(total));
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_decreases() {
        let mut m = Monotonic::default();
        assert_eq!(m.report(50, 100), 50);
        assert_eq!(m.report(40, 100), 50);
        assert_eq!(m.report(60, 100), 60);
    }

    #[test]
    fn shrink_reclamps() {
        let mut m = Monotonic::default();
        assert_eq!(m.report(80, 100), 80);
        // The user unticked a file: the selected total is now smaller than what was reported.
        assert_eq!(m.report(30, 50), 50);
        assert_eq!(m.report(30, 60), 50);
        assert_eq!(m.report(55, 60), 55);
    }
}
