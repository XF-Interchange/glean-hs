//! SysCalls trait — external function interface for the bytecode VM.
//!
//! Rust equivalent of glean/rts/bytecode/syscall.h from Meta Glean.
//!
//! When the VM executes a CallFun_* opcode, it calls into the
//! SysCalls implementation. This decouples the VM from the specific
//! operations it can perform, making it testable in isolation.
//!
//! The actual syscall implementations live in the Glean Haskell layer
//! and will be connected via FFI in Phase 11 (storage integration).

use crate::rts::id::{Id, Pid};
use crate::rts::binary::Output;

/// A bytecode set — an opaque handle to a set of byte strings.
/// Used by the VM for intermediate result collection.
/// The actual implementation is provided by the SysCalls implementor.
pub type BytestringSetHandle = u64;

/// The result of a VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Subroutine completed normally via Ret.
    Done,
    /// Subroutine hit Suspend — execution paused at pc.
    /// Resume by calling vm.execute() again with the same frame.
    Suspended { pc: usize },
}

/// External functions callable from the bytecode VM.
///
/// Implementors provide the actual operations the VM needs —
/// fact lookup, set manipulation, ID renaming, etc.
///
/// This trait is the boundary between the pure Rust VM and
/// the Glean Haskell layer above it.
pub trait SysCalls {
    /// Rename a fact ID — used during fact rebase operations.
    /// Maps an old Id to a new Id in the current database context.
    fn rename(&mut self, id: Id, pid: Pid) -> Id;

    /// Allocate a new empty bytestring set.
    /// Returns an opaque handle.
    fn new_set(&mut self) -> BytestringSetHandle;

    /// Insert the contents of an output buffer into a set.
    fn insert_output_set(
        &mut self,
        set: BytestringSetHandle,
        output: &Output,
    );

    /// Convert a set to an array, writing to an output buffer.
    /// Frees the set after conversion.
    fn set_to_array(
        &mut self,
        set: BytestringSetHandle,
        output: &mut Output,
    );

    /// Free a bytestring set without converting it.
    fn free_set(&mut self, set: BytestringSetHandle);

    /// Look up a fact by predicate and key.
    /// Returns Id::INVALID if not found.
    fn lookup_fact(&self, pid: Pid, key: &[u8]) -> Id;

    /// Get the type (Pid) of a fact by its Id.
    /// Returns Pid::INVALID if not found.
    fn fact_type(&self, id: Id) -> Pid;
}

/// A no-op SysCalls implementation for testing the VM in isolation.
/// All operations are stubs that return safe default values.
pub struct NoOpSysCalls;

impl SysCalls for NoOpSysCalls {
    fn rename(&mut self, id: Id, _pid: Pid) -> Id {
        id // identity rename
    }

    fn new_set(&mut self) -> BytestringSetHandle {
        0
    }

    fn insert_output_set(
        &mut self,
        _set: BytestringSetHandle,
        _output: &Output,
    ) {}

    fn set_to_array(
        &mut self,
        _set: BytestringSetHandle,
        _output: &mut Output,
    ) {}

    fn free_set(&mut self, _set: BytestringSetHandle) {}

    fn lookup_fact(&self, _pid: Pid, _key: &[u8]) -> Id {
        Id::INVALID
    }

    fn fact_type(&self, _id: Id) -> Pid {
        Pid::INVALID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_rename() {
        let mut sc = NoOpSysCalls;
        assert_eq!(sc.rename(Id(42), Pid(1)), Id(42));
    }

    #[test]
    fn test_noop_set_ops() {
        let mut sc = NoOpSysCalls;
        let handle = sc.new_set();
        assert_eq!(handle, 0);
        let out = Output::new();
        sc.insert_output_set(handle, &out);
        let mut out2 = Output::new();
        sc.set_to_array(handle, &mut out2);
        assert!(out2.is_empty()); // noop — nothing written
        sc.free_set(handle);
    }

    #[test]
    fn test_noop_lookup() {
        let sc = NoOpSysCalls;
        assert_eq!(sc.lookup_fact(Pid(1), b"key"), Id::INVALID);
        assert_eq!(sc.fact_type(Id(1024)), Pid::INVALID);
    }

    #[test]
    fn test_exit_reason_eq() {
        assert_eq!(ExitReason::Done, ExitReason::Done);
        assert_eq!(
            ExitReason::Suspended { pc: 42 },
            ExitReason::Suspended { pc: 42 }
        );
        assert_ne!(
            ExitReason::Done,
            ExitReason::Suspended { pc: 0 }
        );
    }
}
