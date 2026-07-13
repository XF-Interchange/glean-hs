//! Define trait — extends Lookup with fact insertion.
//!
//! Rust equivalent of glean/rts/define.h from Meta Glean.

use crate::rts::id::{Id, Pid};
use crate::rts::fact::Clause;
use crate::rts::lookup::Lookup;

/// Abstract write+read interface.
/// A Lookup you can also define (insert) facts into.
pub trait Define: Lookup {
    /// Add a fact and return its ID.
    ///
    /// Returns:
    ///   Some(existing_id) — fact already exists with same key+value
    ///   Some(new_id)      — new fact inserted
    ///   None              — fact exists with same key but different value
    ///
    /// max_ref: highest fact ID referenced by this clause.
    /// Pass None if unknown — always safe.
    fn define(
        &mut self,
        pid:     Pid,
        clause:  Clause<'_>,
        max_ref: Option<Id>,
    ) -> Option<Id>;
}
