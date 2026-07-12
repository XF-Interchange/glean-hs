//! Identity types for Glean facts and predicates.
//!
//! Rust equivalent of glean/rts/id.h from Meta Glean.

/// A fact identifier.
/// Id(0) is INVALID. Valid fact IDs start at LOWEST (1024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Id(pub u64);

/// A predicate identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pid(pub u64);

impl Id {
    pub const INVALID: Id = Id(0);
    pub const LOWEST: Id  = Id(1024);

    #[inline]
    pub fn is_valid(self) -> bool { self.0 != 0 }

    #[inline]
    pub fn valid(self) -> Option<Id> {
        if self.is_valid() { Some(self) } else { None }
    }
}

impl Pid {
    pub const INVALID: Pid = Pid(0);

    #[inline]
    pub fn is_valid(self) -> bool { self.0 != 0 }

    #[inline]
    pub fn valid(self) -> Option<Pid> {
        if self.is_valid() { Some(self) } else { None }
    }
}

impl Id {
    /// Convert to Thrift wire format (i64).
    #[inline]
    pub fn to_thrift(self) -> i64 { self.0 as i64 }

    /// Convert from Thrift wire format (i64).
    #[inline]
    pub fn from_thrift(x: i64) -> Id { Id(x as u64) }

    /// Raw word value (for bytecode VM register marshalling).
    #[inline]
    pub fn to_word(self) -> u64 { self.0 }

    /// From raw word value.
    #[inline]
    pub fn from_word(w: u64) -> Id { Id(w) }

    /// Distance between two Ids (to - from).
    #[inline]
    pub fn distance(from: Id, to: Id) -> u64 { to.0 - from.0 }
}

impl Pid {
    /// Convert to Thrift wire format (i64).
    #[inline]
    pub fn to_thrift(self) -> i64 { self.0 as i64 }

    /// Convert from Thrift wire format (i64).
    #[inline]
    pub fn from_thrift(x: i64) -> Pid { Pid(x as u64) }

    /// Raw word value (for bytecode VM register marshalling).
    #[inline]
    pub fn to_word(self) -> u64 { self.0 }

    /// From raw word value.
    #[inline]
    pub fn from_word(w: u64) -> Pid { Pid(w) }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Id({})", self.0)
    }
}

impl std::fmt::Display for Pid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pid({})", self.0)
    }
}

impl std::ops::Add<u64> for Id {
    type Output = Id;
    fn add(self, rhs: u64) -> Id { Id(self.0 + rhs) }
}

impl std::ops::Add<u64> for Pid {
    type Output = Pid;
    fn add(self, rhs: u64) -> Pid { Pid(self.0 + rhs) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_invalid() {
        assert!(!Id::INVALID.is_valid());
        assert!(Id::INVALID.valid().is_none());
    }

    #[test]
    fn test_id_valid() {
        let id = Id(1024);
        assert!(id.is_valid());
        assert_eq!(id.valid(), Some(id));
    }

    #[test]
    fn test_id_lowest() {
        assert!(Id::LOWEST.is_valid());
        assert_eq!(Id::LOWEST.0, 1024);
    }

    #[test]
    fn test_thrift_roundtrip_id() {
        let id = Id(42);
        assert_eq!(Id::from_thrift(id.to_thrift()), id);
    }

    #[test]
    fn test_thrift_roundtrip_pid() {
        let pid = Pid(7);
        assert_eq!(Pid::from_thrift(pid.to_thrift()), pid);
    }

    #[test]
    fn test_word_roundtrip() {
        let id = Id(999);
        assert_eq!(Id::from_word(id.to_word()), id);
    }

    #[test]
    fn test_distance() {
        assert_eq!(Id::distance(Id(10), Id(15)), 5);
        assert_eq!(Id::distance(Id(1024), Id(1024)), 0);
    }

    #[test]
    fn test_ordering() {
        assert!(Id(1) < Id(2));
        assert!(Id(100) > Id(50));
        assert!(Pid(1) < Pid(2));
    }

    #[test]
    fn test_add() {
        assert_eq!(Id(10) + 5, Id(15));
        assert_eq!(Pid(3) + 1, Pid(4));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Id(42)), "Id(42)");
        assert_eq!(format!("{}", Pid(7)), "Pid(7)");
    }
}
