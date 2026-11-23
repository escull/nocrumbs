// SPDX-License-Identifier: GPL-3.0-or-later
//! Frecency: how often and how recently each path was used, so completion can offer the
//! folder you actually use before the alphabetically first one (brief §5.1).

use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;

const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;

/// How often and when one path was last used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub count: u32,
    /// Seconds since the Unix epoch of the most recent use.
    pub last: i64,
}

impl Visit {
    fn score(&self, now: i64) -> u32 {
        let age = (now - self.last).max(0);
        let weight = if age < HOUR {
            16
        } else if age < DAY {
            8
        } else if age < WEEK {
            4
        } else {
            1
        };
        self.count.saturating_mul(weight)
    }
}

/// A bounded map from full path to [`Visit`]. Paths are canonical, as the [`crate::Fs`] gives them.
///
/// Score = visits x a weight that falls with age: x16 within the hour, x8 within the day, x4
/// within the week, x1 after that. Integers, so ranking is exactly reproducible in tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frecency {
    map: BTreeMap<String, Visit>,
}

impl Frecency {
    /// Retention cap: when exceeded, the least-used, least-recent paths are dropped.
    pub const MAX_ENTRIES: usize = 2_000;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `(path, visit)` in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Visit)> {
        self.map.iter().map(|(p, v)| (p.as_str(), *v))
    }

    /// Rebuild from stored rows (a count of 0 is ignored). Applies the retention cap.
    pub fn from_rows(rows: impl IntoIterator<Item = (String, Visit)>) -> Self {
        let mut f = Self::new();
        for (path, v) in rows {
            if v.count > 0 {
                f.map.insert(path, v);
            }
        }
        f.prune();
        f
    }

    /// `path` was used (navigated to, or selected by typing its path) at time `now`.
    pub fn record(&mut self, path: &str, now: i64) {
        let v = self.map.entry(path.to_string()).or_insert(Visit { count: 0, last: now });
        v.count = v.count.saturating_add(1);
        v.last = v.last.max(now);
        if self.map.len() > Self::MAX_ENTRIES {
            self.prune();
        }
    }

    /// 0 for a path never used.
    pub fn score(&self, path: &str, now: i64) -> u32 {
        self.map.get(path).map_or(0, |v| v.score(now))
    }

    /// The scores of everything used directly inside `dir`, by name. One lookup per completion
    /// instead of one per candidate, which matters in a folder of 100,000 entries.
    pub fn scores_in(&self, dir: &str, now: i64) -> HashMap<&str, u32> {
        let prefix = if dir == "/" { "/".to_string() } else { format!("{dir}/") };
        self.map
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .take_while(|(p, _)| p.starts_with(&prefix))
            .filter_map(|(p, v)| {
                let name = &p[prefix.len()..];
                (!name.is_empty() && !name.contains('/')).then(|| (name, v.score(now)))
            })
            .collect()
    }

    /// `from` was renamed to `to`: carry its history (and that of everything inside it) along.
    pub fn renamed(&mut self, from: &str, to: &str) {
        let prefix = format!("{from}/");
        let moved: Vec<(String, Visit)> = self
            .map
            .iter()
            .filter(|(p, _)| p.as_str() == from || p.starts_with(&prefix))
            .map(|(p, v)| (format!("{to}{}", &p[from.len()..]), *v))
            .collect();
        self.map.retain(|p, _| p != from && !p.starts_with(&prefix));
        self.map.extend(moved);
    }

    pub fn remove(&mut self, path: &str) {
        self.map.remove(path);
    }

    fn prune(&mut self) {
        while self.map.len() > Self::MAX_ENTRIES {
            let weakest = self.map.iter().min_by_key(|(_, v)| (v.count, v.last)).map(|(p, _)| p.clone());
            match weakest {
                Some(p) => {
                    self.map.remove(&p);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_paths_score_zero_and_visits_accumulate() {
        let mut f = Frecency::new();
        assert_eq!(f.score("/a", 100), 0);
        f.record("/a", 100);
        f.record("/a", 100);
        assert_eq!(f.score("/a", 100), 2 * 16, "recent");
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn score_decays_with_age() {
        let mut f = Frecency::new();
        f.record("/a", 0);
        assert_eq!(f.score("/a", 10), 16);
        assert_eq!(f.score("/a", HOUR), 8);
        assert_eq!(f.score("/a", DAY), 4);
        assert_eq!(f.score("/a", WEEK), 1);
        assert_eq!(f.score("/a", 10 * WEEK), 1);
        assert_eq!(f.score("/a", -5), 16, "a clock that went backwards is treated as now");
    }

    #[test]
    fn many_old_visits_can_beat_one_recent_one() {
        let mut f = Frecency::new();
        for _ in 0..20 {
            f.record("/often", 0);
        }
        f.record("/once", 3 * WEEK);
        assert!(f.score("/often", 3 * WEEK) > f.score("/once", 3 * WEEK));
    }

    #[test]
    fn last_visit_never_goes_backwards() {
        let mut f = Frecency::new();
        f.record("/a", 1_000);
        f.record("/a", 10);
        assert_eq!(f.iter().next().unwrap().1.last, 1_000);
    }

    #[test]
    fn rename_carries_history_of_the_folder_and_what_is_inside() {
        let mut f = Frecency::new();
        f.record("/a/old", 1);
        f.record("/a/old/inner.txt", 1);
        f.record("/a/older", 1);
        f.renamed("/a/old", "/a/new");
        let paths: Vec<&str> = f.iter().map(|(p, _)| p).collect();
        assert_eq!(paths, ["/a/new", "/a/new/inner.txt", "/a/older"], "a sibling with the same prefix is untouched");
    }

    #[test]
    fn retention_cap_drops_the_weakest() {
        let mut f = Frecency::new();
        f.record("/keep", 5);
        f.record("/keep", 6);
        for i in 0..Frecency::MAX_ENTRIES + 10 {
            f.record(&format!("/p{i}"), 10 + i as i64);
        }
        assert_eq!(f.len(), Frecency::MAX_ENTRIES);
        assert!(f.score("/keep", 100) > 0, "the twice-used path survives");
    }

    #[test]
    fn scores_in_a_folder_cover_only_its_direct_children() {
        let mut f = Frecency::new();
        f.record("/a/x", 0);
        f.record("/a/y", 0);
        f.record("/a/y", 0);
        f.record("/a/sub/deep", 0);
        f.record("/ab/x", 0);
        f.record("/b", 0);
        let s = f.scores_in("/a", 0);
        assert_eq!(s.len(), 2);
        assert_eq!((s["x"], s["y"]), (16, 32));
        assert_eq!(f.scores_in("/", 0).len(), 1, "only /b is directly in the root");
        assert!(f.scores_in("/nope", 0).is_empty());
    }

    #[test]
    fn rows_with_zero_count_are_ignored() {
        let f = Frecency::from_rows([
            ("/a".to_string(), Visit { count: 0, last: 1 }),
            ("/b".to_string(), Visit { count: 2, last: 1 }),
        ]);
        assert_eq!(f.len(), 1);
    }
}
