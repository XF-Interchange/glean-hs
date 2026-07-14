//! Predicate and Inventory registry.
//!
//! Rust equivalent of glean/rts/inventory.h from Meta Glean.
//!
//! Predicate: schema information + bytecode subroutines for
//!   typechecking and traversal of facts.
//!
//! Inventory: a dense registry of Predicates indexed by Pid.

use std::sync::Arc;
use crate::rts::id::{Id, Pid};
use crate::rts::fact::Clause;
use crate::rts::binary::Output;
use crate::rts::bytecode::vm::Subroutine;
use crate::rts::bytecode::syscall::SysCalls;

/// Information about a single predicate in an open database.
///
/// Contains the schema metadata and compiled bytecode subroutines
/// for typechecking and traversing facts of this predicate type.
///
/// Equivalent to struct Predicate in inventory.h.
#[derive(Debug)]
pub struct Predicate {
    /// The predicate's unique ID.
    pub id:      Pid,
    /// Human-readable name (e.g. "src.File").
    pub name:    String,
    /// Schema version number.
    pub version: i32,
    /// Bytecode subroutine for typechecking a fact's clause.
    /// Validates that key+value match the predicate's schema,
    /// and rewrites fact IDs via the rename syscall.
    pub typechecker: Arc<Subroutine>,
    /// Bytecode subroutine for traversing fact references.
    /// Calls the rename syscall for each fact ID referenced
    /// in the clause.
    pub traverser: Arc<Subroutine>,
}

impl Predicate {
    /// Create a new Predicate with the given subroutines.
    pub fn new(
        id:          Pid,
        name:        impl Into<String>,
        version:     i32,
        typechecker: Subroutine,
        traverser:   Subroutine,
    ) -> Self {
        Predicate {
            id,
            name:        name.into(),
            version,
            typechecker: Arc::new(typechecker),
            traverser:   Arc::new(traverser),
        }
    }

    /// Run the typechecker subroutine on a fact clause.
    ///
    /// The typechecker validates the clause structure and
    /// rewrites fact IDs via the rename syscall.
    /// Returns the canonical output (key + value) on success.
    pub fn typecheck<S: SysCalls>(
        &self,
        syscalls: &mut S,
        clause:   Clause<'_>,
    ) -> Result<(Vec<u8>, usize), String> {
        let mut frame = self.typechecker.new_frame();

        // Set up input registers:
        // The typechecker expects pointers to the clause data
        // in its input registers, as raw u64 pointer values.
        // reg[0] = ptr to clause start (data)
        // reg[1] = ptr to key end / value start
        // reg[2] = ptr to clause end
        // reg[n_inputs-n_outputs .. n_inputs) = output buffer ptrs
        // (handled by the frame's output buffers)

        // For now, provide the output buffer to collect results.
        // The actual pointer setup depends on the Haskell FFI
        // calling convention — stubbed for Phase 9.
        let _ = clause; // used in Phase 11 with full FFI

        use crate::rts::bytecode::syscall::ExitReason;
        match self.typechecker.execute(&mut frame, syscalls) {
            Ok(ExitReason::Done) => {
                let key_size = if frame.n_outputs > 0 && frame.regs.len() > 0 { frame.reg(0) as usize } else { 0 };
                let output = if frame.n_outputs > 0 { frame.output(0).as_bytes().to_vec() } else { Vec::new() };
                Ok((output, key_size))
            }
            Ok(ExitReason::Suspended { pc }) => {
                Err(format!("typechecker suspended unexpectedly at pc={}", pc))
            }
            Err(e) => Err(format!("typechecker error: {:?}", e)),
        }
    }

    /// Run the traverser subroutine on a fact clause.
    ///
    /// Calls f for each fact ID referenced in the clause.
    /// Used during ownership propagation and garbage collection.
    pub fn traverse<S: SysCalls>(
        &self,
        syscalls: &mut S,
        clause:   Clause<'_>,
        mut f:    impl FnMut(Id, Pid),
    ) -> Result<(), String> {
        let _ = (clause, &mut f); // used in Phase 11 with full FFI

        use crate::rts::bytecode::syscall::ExitReason;
        let mut frame = self.traverser.new_frame();
        match self.traverser.execute(&mut frame, syscalls) {
            Ok(ExitReason::Done) => Ok(()),
            Ok(ExitReason::Suspended { pc }) => {
                Err(format!("traverser suspended unexpectedly at pc={}", pc))
            }
            Err(e) => Err(format!("traverser error: {:?}", e)),
        }
    }
}

impl PartialEq for Predicate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.version == other.version
    }
}

impl Eq for Predicate {}

/// A registry of predicates indexed by Pid.
///
/// Dense array — gaps are allowed but cost O(max_pid - min_pid) space.
/// Equivalent to struct Inventory in inventory.h.
pub struct Inventory {
    /// The Pid of the first predicate.
    first_id: Pid,
    /// Dense array of predicates. None = gap (no predicate at that Pid).
    preds: Vec<Option<Predicate>>,
}

impl Inventory {
    /// Create an empty Inventory.
    pub fn new() -> Self {
        Inventory {
            first_id: Pid::INVALID,
            preds:    Vec::new(),
        }
    }

    /// Create an Inventory from a list of predicates.
    /// The predicates do not need to be sorted or contiguous.
    pub fn from_predicates(predicates: Vec<Predicate>) -> Self {
        if predicates.is_empty() {
            return Inventory::new();
        }

        let min_pid = predicates.iter().map(|p| p.id.0).min().unwrap();
        let max_pid = predicates.iter().map(|p| p.id.0).max().unwrap();

        let first_id = Pid(min_pid);
        let size     = (max_pid - min_pid + 1) as usize;
        let mut preds: Vec<Option<Predicate>> = (0..size).map(|_| None).collect();

        for pred in predicates {
            let index = (pred.id.0 - min_pid) as usize;
            preds[index] = Some(pred);
        }

        Inventory { first_id, preds }
    }

    /// Look up a predicate by Pid.
    /// Returns None if no predicate exists with that Pid.
    pub fn lookup(&self, pid: Pid) -> Option<&Predicate> {
        if self.first_id == Pid::INVALID || pid < self.first_id {
            return None;
        }
        let index = (pid.0 - self.first_id.0) as usize;
        self.preds.get(index)?.as_ref()
    }

    /// The Pid of the first predicate.
    pub fn first_id(&self) -> Pid {
        self.first_id
    }

    /// The next Pid after the last predicate (exclusive upper bound).
    pub fn first_free_id(&self) -> Pid {
        if self.first_id == Pid::INVALID {
            return Pid::INVALID;
        }
        self.first_id + self.preds.len() as u64
    }

    /// Return all predicates (skipping gaps).
    pub fn predicates(&self) -> impl Iterator<Item = &Predicate> {
        self.preds.iter().filter_map(|p| p.as_ref())
    }

    /// Return the number of predicates (excluding gaps).
    pub fn len(&self) -> usize {
        self.preds.iter().filter(|p| p.is_some()).count()
    }

    /// Return true if the inventory contains no predicates.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory::new()
    }
}

/// Helper to create a minimal stub Subroutine for testing.
/// Just executes Ret immediately.
#[cfg(test)]
fn stub_subroutine() -> Subroutine {
    use crate::rts::bytecode::opcode::Op;
    Subroutine::new(
        vec![Op::Ret.to_u8() as u64],
        0, 1, 0, vec![], vec![],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_predicate(pid: u64, name: &str) -> Predicate {
        Predicate::new(
            Pid(pid),
            name,
            1,
            stub_subroutine(),
            stub_subroutine(),
        )
    }

    #[test]
    fn test_predicate_new() {
        let p = make_predicate(1, "src.File");
        assert_eq!(p.id,      Pid(1));
        assert_eq!(p.name,    "src.File");
        assert_eq!(p.version, 1);
    }

    #[test]
    fn test_inventory_empty() {
        let inv = Inventory::new();
        assert!(inv.is_empty());
        assert_eq!(inv.len(), 0);
        assert_eq!(inv.lookup(Pid(1)), None);
    }

    #[test]
    fn test_inventory_single() {
        let p = make_predicate(5, "src.File");
        let inv = Inventory::from_predicates(vec![p]);
        assert_eq!(inv.len(), 1);
        assert_eq!(inv.first_id(), Pid(5));
        assert_eq!(inv.first_free_id(), Pid(6));

        let found = inv.lookup(Pid(5)).unwrap();
        assert_eq!(found.id,   Pid(5));
        assert_eq!(found.name, "src.File");
    }

    #[test]
    fn test_inventory_multiple() {
        let preds = vec![
            make_predicate(1, "src.File"),
            make_predicate(2, "src.Directory"),
            make_predicate(3, "src.FileLines"),
        ];
        let inv = Inventory::from_predicates(preds);
        assert_eq!(inv.len(), 3);
        assert_eq!(inv.first_id(),      Pid(1));
        assert_eq!(inv.first_free_id(), Pid(4));

        assert!(inv.lookup(Pid(1)).is_some());
        assert!(inv.lookup(Pid(2)).is_some());
        assert!(inv.lookup(Pid(3)).is_some());
        assert!(inv.lookup(Pid(4)).is_none());
        assert!(inv.lookup(Pid(0)).is_none());
    }

    #[test]
    fn test_inventory_with_gaps() {
        let preds = vec![
            make_predicate(1, "pred.A"),
            make_predicate(3, "pred.C"), // gap at 2
            make_predicate(5, "pred.E"), // gap at 4
        ];
        let inv = Inventory::from_predicates(preds);
        assert_eq!(inv.len(), 3);
        assert_eq!(inv.first_id(),      Pid(1));
        assert_eq!(inv.first_free_id(), Pid(6));

        assert!(inv.lookup(Pid(1)).is_some());
        assert!(inv.lookup(Pid(2)).is_none()); // gap
        assert!(inv.lookup(Pid(3)).is_some());
        assert!(inv.lookup(Pid(4)).is_none()); // gap
        assert!(inv.lookup(Pid(5)).is_some());
    }

    #[test]
    fn test_inventory_predicates_iter() {
        let preds = vec![
            make_predicate(1, "pred.A"),
            make_predicate(2, "pred.B"),
        ];
        let inv = Inventory::from_predicates(preds);
        let names: Vec<&str> = inv.predicates()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"pred.A"));
        assert!(names.contains(&"pred.B"));
    }

    #[test]
    fn test_predicate_typecheck_stub() {
        // With a stub subroutine (just Ret), typecheck should complete
        let p = make_predicate(1, "test.Pred");
        let data = b"key\x00val";
        let clause = Clause::new(data, 3, 3);
        let mut sc = crate::rts::bytecode::syscall::NoOpSysCalls;
        // Stub typechecker just returns Ret — output is empty
        let result = p.typecheck(&mut sc, clause);
        assert!(result.is_ok());
    }

    #[test]
    fn test_predicate_traverse_stub() {
        let p = make_predicate(1, "test.Pred");
        let data = b"key\x00val";
        let clause = Clause::new(data, 3, 3);
        let mut sc = crate::rts::bytecode::syscall::NoOpSysCalls;
        let mut count = 0usize;
        let result = p.traverse(&mut sc, clause, |_, _| { count += 1; });
        assert!(result.is_ok());
        assert_eq!(count, 0); // stub traverser calls nothing
    }
}
