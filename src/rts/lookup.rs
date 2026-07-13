//! Lookup trait — the read interface for Glean facts.
//!
//! Rust equivalent of glean/rts/lookup.h from Meta Glean.
//!
//! Uses &mut dyn FnMut for callbacks instead of generics
//! to ensure dyn compatibility (Box<dyn Lookup> works).

use crate::rts::id::{Id, Pid};
use crate::rts::fact::{Clause, FactRef};

/// An interval [start, end) of fact IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: Id,
    pub end:   Id,
}

impl Interval {
    pub fn new(start: Id, end: Id) -> Self {
        Interval { start, end }
    }

    pub fn empty() -> Self {
        Interval { start: Id::INVALID, end: Id::INVALID }
    }

    pub fn count(&self) -> u64 {
        if self.end > self.start {
            Id::distance(self.start, self.end)
        } else {
            0
        }
    }
}

impl std::ops::Add for Interval {
    type Output = Interval;
    fn add(self, rhs: Interval) -> Interval {
        Interval {
            start: self.start,
            end:   rhs.end,
        }
    }
}

/// Abstract read interface for looking up facts.
/// All callbacks use &mut dyn FnMut for dyn compatibility.
pub trait Lookup {
    /// Look up a fact ID by predicate and key.
    fn id_by_key(&self, pid: Pid, key: &[u8]) -> Option<Id>;

    /// Look up a predicate ID by fact ID.
    fn type_by_id(&self, id: Id) -> Option<Pid>;

    /// Look up a fact by ID, passing it to a callback.
    /// Returns true if the fact was found.
    fn fact_by_id(&self, id: Id, f: &mut dyn FnMut(Pid, Clause<'_>)) -> bool;

    /// The lowest fact ID in this lookup.
    fn starting_id(&self) -> Id;

    /// The next available fact ID (one past the highest).
    fn first_free_id(&self) -> Id;

    /// Count of facts for a given predicate.
    fn count(&self, pid: Pid) -> Interval;

    /// Enumerate all facts, calling f for each.
    fn enumerate_all(&self, f: &mut dyn FnMut(FactRef<'_>));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_count() {
        let iv = Interval::new(Id(10), Id(15));
        assert_eq!(iv.count(), 5);
    }

    #[test]
    fn test_interval_empty() {
        let iv = Interval::empty();
        assert_eq!(iv.count(), 0);
    }

    #[test]
    fn test_interval_add() {
        let a = Interval::new(Id(1), Id(5));
        let b = Interval::new(Id(5), Id(10));
        let c = a + b;
        assert_eq!(c.start, Id(1));
        assert_eq!(c.end,   Id(10));
    }
}
