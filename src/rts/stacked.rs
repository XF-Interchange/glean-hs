//! Stacked<T> — a two-layer database view.
//!
//! Rust equivalent of glean/rts/stacked.h from Meta Glean.
//!
//! Stacks a base Lookup with a stacked Lookup or Define.
//! Facts with id < mid come from base.
//! Facts with id >= mid come from stacked.

use crate::rts::id::{Id, Pid};
use crate::rts::fact::{Clause, FactRef};
use crate::rts::lookup::{Lookup, Interval};
use crate::rts::define::Define;

/// A two-layer view: base Lookup + stacked T.
/// mid = stacked.starting_id() — the boundary between layers.
pub struct Stacked<T> {
    base:    Box<dyn Lookup>,
    stacked: T,
    mid:     Id,
}

impl<T: Lookup> Stacked<T> {
    /// Create a new Stacked view.
    pub fn new(base: Box<dyn Lookup>, stacked: T) -> Self {
        let mid = stacked.starting_id();
        Stacked { base, stacked, mid }
    }
}

impl<T: Lookup> Lookup for Stacked<T> {
    fn id_by_key(&self, pid: Pid, key: &[u8]) -> Option<Id> {
        // Check stacked first
        if let Some(id) = self.stacked.id_by_key(pid, key) {
            return Some(id);
        }
        // Then base — only if id is below mid
        let id = self.base.id_by_key(pid, key)?;
        if id < self.mid { Some(id) } else { None }
    }

    fn type_by_id(&self, id: Id) -> Option<Pid> {
        if id < self.mid {
            self.base.type_by_id(id)
        } else {
            self.stacked.type_by_id(id)
        }
    }

    fn fact_by_id(&self, id: Id, f: &mut dyn FnMut(Pid, Clause<'_>)) -> bool {
        if id < self.mid {
            self.base.fact_by_id(id, f)
        } else {
            self.stacked.fact_by_id(id, f)
        }
    }

    fn starting_id(&self) -> Id {
        self.base.starting_id()
    }

    fn first_free_id(&self) -> Id {
        self.stacked.first_free_id()
    }

    fn count(&self, pid: Pid) -> Interval {
        self.base.count(pid) + self.stacked.count(pid)
    }

    fn enumerate_all(&self, f: &mut dyn FnMut(FactRef<'_>)) {
        self.base.enumerate_all(f);
        self.stacked.enumerate_all(f);
    }
}

impl<T: Define> Define for Stacked<T> {
    fn define(
        &mut self,
        pid:     Pid,
        clause:  Clause<'_>,
        max_ref: Option<Id>,
    ) -> Option<Id> {
        // If max_ref is below mid, fact may exist in base
        if max_ref.map_or(false, |r| r < self.mid) {
            if let Some(id) = self.base.id_by_key(pid, clause.key()) {
                if id < self.mid {
                    if clause.value_size() == 0 {
                        return Some(id);
                    }
                    let mut matches = false;
                    self.base.fact_by_id(id, &mut |_, found| {
                        matches = clause.value() == found.value();
                    });
                    return if matches { Some(id) } else { None };
                }
            }
        }
        self.stacked.define(pid, clause, max_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stacked_interval_add() {
        let a = Interval::new(Id(1), Id(5));
        let b = Interval::new(Id(5), Id(10));
        let c = a + b;
        // Combined interval Id(1)..Id(10) spans 9 facts
        assert_eq!(c.start, Id(1));
        assert_eq!(c.end,   Id(10));
        assert_eq!(c.count(), 9);
    }
}
