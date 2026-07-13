//! Fact types for Glean's fact storage.
//!
//! Rust equivalent of glean/rts/fact.h from Meta Glean.
//!
//! A Fact is Glean's fundamental unit of data:
//!   [ Fact header | key bytes | value bytes ]
//!
//! All three parts live in one contiguous allocation
//! for cache efficiency.
//!
//! Clause is a borrowed view into fact data.
//! FactRef is a borrowed reference to a fact's id, type, and data.

use crate::rts::id::{Id, Pid};
use crate::rts::binary::{Input, Output};

/// A stored fact — the fundamental unit of data in Glean.
///
/// Contains:
///   id:         unique fact identifier
///   pid:        predicate identifier (which schema this fact belongs to)
///   key_size:   size of the key portion of data
///   value_size: size of the value portion of data
///   data:       key bytes followed immediately by value bytes
pub struct Fact {
    pub id:         Id,
    pub pid:        Pid,
    pub key_size:   u32,
    pub value_size: u32,
    data:           Box<[u8]>,
}

impl Fact {
    /// Create a new Fact with the given id, predicate, key and value.
    pub fn new(id: Id, pid: Pid, key: &[u8], value: &[u8]) -> Self {
        let mut data = Vec::with_capacity(key.len() + value.len());
        data.extend_from_slice(key);
        data.extend_from_slice(value);
        Fact {
            id,
            pid,
            key_size:   key.len() as u32,
            value_size: value.len() as u32,
            data:       data.into_boxed_slice(),
        }
    }

    /// Return the key bytes.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.data[..self.key_size as usize]
    }

    /// Return the value bytes.
    #[inline]
    pub fn value(&self) -> &[u8] {
        &self.data[self.key_size as usize..]
    }

    /// Return all data bytes (key + value).
    #[inline]
    pub fn all_data(&self) -> &[u8] {
        &self.data
    }

    /// Total size of fact data in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Return a borrowed Clause view into this fact's data.
    #[inline]
    pub fn clause(&self) -> Clause<'_> {
        Clause {
            data:       &self.data,
            key_size:   self.key_size,
            value_size: self.value_size,
        }
    }

    /// Return a FactRef borrowing this fact.
    #[inline]
    pub fn as_ref(&self) -> FactRef<'_> {
        FactRef {
            id:     self.id,
            pid:    self.pid,
            clause: self.clause(),
        }
    }
}

/// A borrowed view into fact data (key + value).
/// Does not own the data — lifetime tied to the source.
///
/// Equivalent to Fact::Clause in fact.h.
#[derive(Clone, Copy)]
pub struct Clause<'a> {
    data:       &'a [u8],
    key_size:   u32,
    value_size: u32,
}

impl<'a> Clause<'a> {
    /// Create a Clause from raw parts.
    #[inline]
    pub fn new(data: &'a [u8], key_size: u32, value_size: u32) -> Self {
        Clause { data, key_size, value_size }
    }

    /// Create a Clause from key bytes only (empty value).
    #[inline]
    pub fn from_key(key: &'a [u8]) -> Self {
        Clause {
            data:       key,
            key_size:   key.len() as u32,
            value_size: 0,
        }
    }

    /// Return the key bytes.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.data[..self.key_size as usize]
    }

    /// Return the value bytes.
    #[inline]
    pub fn value(&self) -> &[u8] {
        &self.data[self.key_size as usize..]
    }

    /// Return all bytes (key + value).
    #[inline]
    pub fn all_data(&self) -> &[u8] {
        self.data
    }

    /// Total size of this clause in bytes.
    #[inline]
    pub fn size(&self) -> usize {
        self.key_size as usize + self.value_size as usize
    }

    pub fn key_size(&self)   -> u32 { self.key_size }
    pub fn value_size(&self) -> u32 { self.value_size }
}

/// A borrowed reference to a fact — id, predicate, and data.
///
/// Equivalent to Fact::Ref in fact.h.
#[derive(Clone, Copy)]
pub struct FactRef<'a> {
    pub id:     Id,
    pub pid:    Pid,
    pub clause: Clause<'a>,
}

impl<'a> FactRef<'a> {
    /// Create a FactRef from parts.
    #[inline]
    pub fn new(id: Id, pid: Pid, clause: Clause<'a>) -> Self {
        FactRef { id, pid, clause }
    }

    /// Convenience: return the key bytes.
    #[inline]
    pub fn key(&self) -> &[u8] {
        self.clause.key()
    }

    /// Convenience: return the value bytes.
    #[inline]
    pub fn value(&self) -> &[u8] {
        self.clause.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_new() {
        let fact = Fact::new(Id(1024), Pid(1), b"mykey", b"myvalue");
        assert_eq!(fact.id,   Id(1024));
        assert_eq!(fact.pid,  Pid(1));
        assert_eq!(fact.key(),   b"mykey");
        assert_eq!(fact.value(), b"myvalue");
        assert_eq!(fact.size(),  12); // 5 + 7
    }

    #[test]
    fn test_fact_empty_value() {
        let fact = Fact::new(Id(1), Pid(2), b"key", b"");
        assert_eq!(fact.key(),   b"key");
        assert_eq!(fact.value(), b"");
        assert_eq!(fact.size(),  3);
    }

    #[test]
    fn test_fact_empty_key() {
        let fact = Fact::new(Id(1), Pid(2), b"", b"value");
        assert_eq!(fact.key(),   b"");
        assert_eq!(fact.value(), b"value");
    }

    #[test]
    fn test_clause_from_fact() {
        let fact = Fact::new(Id(1), Pid(1), b"k", b"v");
        let clause = fact.clause();
        assert_eq!(clause.key(),   b"k");
        assert_eq!(clause.value(), b"v");
        assert_eq!(clause.size(),  2);
    }

    #[test]
    fn test_clause_from_key() {
        let clause = Clause::from_key(b"lookup_key");
        assert_eq!(clause.key(),        b"lookup_key");
        assert_eq!(clause.value(),      b"");
        assert_eq!(clause.value_size(), 0);
    }

    #[test]
    fn test_fact_ref() {
        let fact = Fact::new(Id(42), Pid(7), b"key", b"val");
        let fref = fact.as_ref();
        assert_eq!(fref.id,  Id(42));
        assert_eq!(fref.pid, Pid(7));
        assert_eq!(fref.key(),   b"key");
        assert_eq!(fref.value(), b"val");
    }

    #[test]
    fn test_clause_copy() {
        // Clause is Copy — verify we can copy it
        let fact = Fact::new(Id(1), Pid(1), b"key", b"val");
        let c1 = fact.clause();
        let c2 = c1; // copy
        assert_eq!(c1.key(), c2.key());
    }

    #[test]
    fn test_fact_ref_copy() {
        let fact = Fact::new(Id(1), Pid(1), b"key", b"val");
        let r1 = fact.as_ref();
        let r2 = r1; // copy
        assert_eq!(r1.id, r2.id);
    }
}
