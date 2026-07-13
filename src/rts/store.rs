//! Store trait — the write interface for Glean facts.
//!
//! Rust equivalent of glean/rts/store.h from Meta Glean.

use crate::rts::fact::FactRef;

/// Abstract write interface for inserting facts.
pub trait Store {
    fn insert(&mut self, fact: FactRef<'_>);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rts::id::{Id, Pid};
    use crate::rts::fact::Fact;

    struct VecStore(Vec<(Id, Pid, Vec<u8>, Vec<u8>)>);

    impl Store for VecStore {
        fn insert(&mut self, fact: FactRef<'_>) {
            self.0.push((
                fact.id,
                fact.pid,
                fact.key().to_vec(),
                fact.value().to_vec(),
            ));
        }
    }

    #[test]
    fn test_store_insert() {
        let mut store = VecStore(Vec::new());
        let fact = Fact::new(Id(1024), Pid(1), b"key", b"val");
        store.insert(fact.as_ref());
        assert_eq!(store.0.len(), 1);
        assert_eq!(store.0[0].0, Id(1024));
        assert_eq!(store.0[0].2, b"key");
        assert_eq!(store.0[0].3, b"val");
    }

    #[test]
    fn test_store_multiple_inserts() {
        let mut store = VecStore(Vec::new());
        for i in 0..5u64 {
            let fact = Fact::new(Id(1024 + i), Pid(1), b"key", b"val");
            store.insert(fact.as_ref());
        }
        assert_eq!(store.0.len(), 5);
    }
}
