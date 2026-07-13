//! FactSet — the central two-index data structure.
//!
//! Rust equivalent of glean/rts/factset.h from Meta Glean.
//!
//! Maintains two indexes over a set of facts:
//!   1. By Id  — Vec<Box<Fact>> in insertion order
//!   2. By Key — HashMap<Pid, HashMap<Vec<u8>, usize>>
//!               maps (predicate, key) → index into facts vec
//!
//! Implements both Lookup (read) and Define (write).

use std::collections::HashMap;
use crate::rts::id::{Id, Pid};
use crate::rts::fact::{Fact, Clause, FactRef};
use crate::rts::lookup::{Lookup, Interval};
use crate::rts::define::Define;
use crate::rts::binary::{Input, Output};

/// A set of facts with two indexes for efficient lookup.
pub struct FactSet {
    /// Base ID — the ID of the first fact in this set.
    starting_id: Id,
    /// Facts in insertion order. Index i = Id(starting_id + i).
    facts: Vec<Box<Fact>>,
    /// Secondary index: (Pid, key_bytes) → index into facts vec.
    keys: HashMap<Pid, HashMap<Vec<u8>, usize>>,
}

impl FactSet {
    /// Create a new empty FactSet.
    pub fn new(starting_id: Id) -> Self {
        FactSet {
            starting_id,
            facts: Vec::new(),
            keys:  HashMap::new(),
        }
    }

    /// Create a FactSet starting at the standard lowest ID.
    pub fn new_from_lowest() -> Self {
        Self::new(Id::LOWEST)
    }

    /// Return the number of facts in this set.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Return true if this set contains no facts.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Compute the Id for a given index into the facts vec.
    #[inline]
    fn id_for_index(&self, index: usize) -> Id {
        self.starting_id + index as u64
    }

    /// Compute the index for a given Id.
    #[inline]
    fn index_for_id(&self, id: Id) -> Option<usize> {
        if id < self.starting_id {
            return None;
        }
        let index = Id::distance(self.starting_id, id) as usize;
        if index < self.facts.len() {
            Some(index)
        } else {
            None
        }
    }

    /// Serialize all facts to binary format.
    /// Format per fact:
    ///   nat(pid)
    ///   nat(key_size)
    ///   nat(value_size)
    ///   key_bytes
    ///   value_bytes
    pub fn serialize(&self) -> Serialized {
        let mut out = Output::new();
        for fact in &self.facts {
            out.packed_nat(fact.pid.0);
            out.packed_nat(fact.key_size as u64);
            out.packed_nat(fact.value_size as u64);
            out.bytes(fact.key());
            out.bytes(fact.value());
        }
        Serialized {
            first_id: self.starting_id,
            count:    self.facts.len(),
            data:     out.into_vec(),
        }
    }

    /// Deserialize facts from binary format into this FactSet.
    /// Returns number of facts deserialized.
    pub fn deserialize(&mut self, serialized: &Serialized) -> usize {
        let mut input = Input::new(&serialized.data);
        let mut count = 0;

        while !input.is_empty() {
            let pid        = Pid(input.packed_nat());
            let key_size   = input.packed_usize();
            let value_size = input.packed_usize();
            let key        = input.bytes(key_size).to_vec();
            let value      = input.bytes(value_size).to_vec();

            let index = self.facts.len();
            let id    = self.id_for_index(index);
            let fact  = Box::new(Fact::new(id, pid, &key, &value));

            self.keys
                .entry(pid)
                .or_insert_with(HashMap::new)
                .insert(key, index);
            self.facts.push(fact);
            count += 1;
        }
        count
    }
}

/// Serialized representation of a FactSet.
pub struct Serialized {
    pub first_id: Id,
    pub count:    usize,
    pub data:     Vec<u8>,
}

impl Lookup for FactSet {
    fn id_by_key(&self, pid: Pid, key: &[u8]) -> Option<Id> {
        let index = self.keys.get(&pid)?.get(key)?;
        Some(self.id_for_index(*index))
    }

    fn type_by_id(&self, id: Id) -> Option<Pid> {
        let index = self.index_for_id(id)?;
        Some(self.facts[index].pid)
    }

    fn fact_by_id(&self, id: Id, f: &mut dyn FnMut(Pid, Clause<'_>)) -> bool {
        match self.index_for_id(id) {
            None => false,
            Some(index) => {
                let fact = &self.facts[index];
                f(fact.pid, fact.clause());
                true
            }
        }
    }

    fn starting_id(&self) -> Id {
        self.starting_id
    }

    fn first_free_id(&self) -> Id {
        self.starting_id + self.facts.len() as u64
    }

    fn count(&self, pid: Pid) -> Interval {
        match self.keys.get(&pid) {
            None    => Interval::empty(),
            Some(m) => Interval::new(
                self.starting_id,
                self.starting_id + m.len() as u64,
            ),
        }
    }

    fn enumerate_all(&self, f: &mut dyn FnMut(FactRef<'_>)) {
        for (index, fact) in self.facts.iter().enumerate() {
            f(FactRef::new(
                self.id_for_index(index),
                fact.pid,
                fact.clause(),
            ));
        }
    }
}

impl Define for FactSet {
    fn define(
        &mut self,
        pid:      Pid,
        clause:   Clause<'_>,
        _max_ref: Option<Id>,
    ) -> Option<Id> {
        let key = clause.key().to_vec();

        // Check if fact already exists with this key
        if let Some(&index) = self.keys
            .get(&pid)
            .and_then(|m| m.get(&key))
        {
            let existing = &self.facts[index];
            if existing.value() == clause.value() {
                // Identical fact — return existing id
                return Some(self.id_for_index(index));
            } else {
                // Same key, different value — conflict
                return None;
            }
        }

        // New fact — insert it
        let index = self.facts.len();
        let id    = self.id_for_index(index);

        let fact = Box::new(Fact::new(
            id,
            pid,
            clause.key(),
            clause.value(),
        ));

        self.facts.push(fact);
        self.keys
            .entry(pid)
            .or_insert_with(HashMap::new)
            .insert(key, index);

        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factset_new() {
        let fs = FactSet::new(Id::LOWEST);
        assert!(fs.is_empty());
        assert_eq!(fs.len(), 0);
        assert_eq!(fs.starting_id(), Id::LOWEST);
        assert_eq!(fs.first_free_id(), Id::LOWEST);
    }

    #[test]
    fn test_define_new_fact() {
        let mut fs = FactSet::new(Id::LOWEST);
        let clause = Clause::new(b"key\x00val", 3, 3);
        let id = fs.define(Pid(1), clause, None);
        assert!(id.is_some());
        assert_eq!(id.unwrap(), Id::LOWEST);
        assert_eq!(fs.len(), 1);
    }

    #[test]
    fn test_define_idempotent() {
        let mut fs = FactSet::new(Id::LOWEST);
        let data = b"key\x00val";
        let clause = Clause::new(data, 3, 3);
        let id1 = fs.define(Pid(1), clause, None);

        let clause2 = Clause::new(data, 3, 3);
        let id2 = fs.define(Pid(1), clause2, None);

        // Same fact twice → same id returned
        assert_eq!(id1, id2);
        assert_eq!(fs.len(), 1); // only one fact stored
    }

    #[test]
    fn test_define_conflict() {
        let mut fs = FactSet::new(Id::LOWEST);
        // Same key, different value → conflict
        let data1 = b"key\x00val1";
        let data2 = b"key\x00val2";
        let clause1 = Clause::new(data1, 3, 4);
        let clause2 = Clause::new(data2, 3, 4);

        fs.define(Pid(1), clause1, None);
        let result = fs.define(Pid(1), clause2, None);
        assert!(result.is_none()); // conflict
    }

    #[test]
    fn test_id_by_key() {
        let mut fs = FactSet::new(Id::LOWEST);
        let data = b"mykey\x00myval";
        let clause = Clause::new(data, 5, 5);
        let id = fs.define(Pid(1), clause, None).unwrap();

        assert_eq!(fs.id_by_key(Pid(1), b"mykey"), Some(id));
        assert_eq!(fs.id_by_key(Pid(1), b"notexist"), None);
        assert_eq!(fs.id_by_key(Pid(2), b"mykey"), None);
    }

    #[test]
    fn test_type_by_id() {
        let mut fs = FactSet::new(Id::LOWEST);
        let data = b"key\x00val";
        let clause = Clause::new(data, 3, 3);
        let id = fs.define(Pid(7), clause, None).unwrap();

        assert_eq!(fs.type_by_id(id), Some(Pid(7)));
        assert_eq!(fs.type_by_id(Id(9999)), None);
    }

    #[test]
    fn test_fact_by_id() {
        let mut fs = FactSet::new(Id::LOWEST);
        let data = b"key\x00val";
        let clause = Clause::new(data, 3, 3);
        let id = fs.define(Pid(1), clause, None).unwrap();

        let mut found_key = Vec::new();
        let found = fs.fact_by_id(id, &mut |_pid, clause| {
            found_key.extend_from_slice(clause.key());
        });
        assert!(found);
        assert_eq!(found_key, b"key");

        let not_found = fs.fact_by_id(Id(9999), &mut |_, _| {});
        assert!(!not_found);
    }

    #[test]
    fn test_enumerate_all() {
        let mut fs = FactSet::new(Id::LOWEST);
        for i in 0..3u64 {
            let key = format!("key{}", i);
            let val = format!("val{}", i);
            let mut data = key.as_bytes().to_vec();
            data.extend_from_slice(val.as_bytes());
            let clause = Clause::new(&data, key.len() as u32, val.len() as u32);
            // We need owned data — use define with a temp buffer
            let _ = fs.define(Pid(1), Clause::new(
                Box::leak(data.into_boxed_slice()),
                key.len() as u32,
                val.len() as u32,
            ), None);
        }
        let mut count = 0;
        fs.enumerate_all(&mut |_| { count += 1; });
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count() {
        let mut fs = FactSet::new(Id::LOWEST);
        let data1 = b"k1\x00v1";
        let data2 = b"k2\x00v2";
        fs.define(Pid(1), Clause::new(data1, 2, 2), None);
        fs.define(Pid(1), Clause::new(data2, 2, 2), None);
        fs.define(Pid(2), Clause::new(data1, 2, 2), None);

        assert_eq!(fs.count(Pid(1)).count(), 2);
        assert_eq!(fs.count(Pid(2)).count(), 1);
        assert_eq!(fs.count(Pid(3)).count(), 0);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut fs1 = FactSet::new(Id::LOWEST);
        let data1 = b"key1\x00val1";
        let data2 = b"key2\x00val2";
        fs1.define(Pid(1), Clause::new(data1, 4, 4), None);
        fs1.define(Pid(2), Clause::new(data2, 4, 4), None);

        let serialized = fs1.serialize();
        assert_eq!(serialized.count, 2);
        assert_eq!(serialized.first_id, Id::LOWEST);

        let mut fs2 = FactSet::new(Id::LOWEST);
        let count = fs2.deserialize(&serialized);
        assert_eq!(count, 2);
        assert_eq!(fs2.len(), 2);

        // Verify facts roundtripped correctly
        assert_eq!(fs2.id_by_key(Pid(1), b"key1"), Some(Id::LOWEST));
        assert_eq!(fs2.id_by_key(Pid(2), b"key2"), Some(Id::LOWEST + 1));
    }

    #[test]
    fn test_multiple_predicates() {
        let mut fs = FactSet::new(Id::LOWEST);
        // Same key, different predicates = different facts
        let data = b"key\x00val";
        let id1 = fs.define(Pid(1), Clause::new(data, 3, 3), None);
        let id2 = fs.define(Pid(2), Clause::new(data, 3, 3), None);

        assert!(id1.is_some());
        assert!(id2.is_some());
        assert_ne!(id1.unwrap(), id2.unwrap());
        assert_eq!(fs.len(), 2);
    }

    #[test]
    fn test_first_free_id_increments() {
        let mut fs = FactSet::new(Id::LOWEST);
        assert_eq!(fs.first_free_id(), Id::LOWEST);

        let data = b"k\x00v";
        fs.define(Pid(1), Clause::new(data, 1, 1), None);
        assert_eq!(fs.first_free_id(), Id::LOWEST + 1);

        fs.define(Pid(2), Clause::new(data, 1, 1), None);
        assert_eq!(fs.first_free_id(), Id::LOWEST + 2);
    }
}
