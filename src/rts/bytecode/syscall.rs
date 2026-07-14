//! SysCalls trait — external function interface for the bytecode VM.
//!
//! Rust equivalent of glean/rts/bytecode/syscall.h from Meta Glean.
//! Syscall lists verified against glean/bytecode/Glean/Bytecode/SysCalls.hs.
//!
//! Two syscall contexts:
//!   typecheckSysCalls — used during fact typechecking
//!   userQuerySysCalls — used during query execution (Phase 11+)

use crate::rts::id::{Id, Pid};
use crate::rts::binary::Output;

/// Opaque handle to a bytestring set.
pub type BytestringSetHandle = u64;

/// Opaque handle to a word set (integer set).
pub type WordSetHandle = u64;

/// The result of a VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// Subroutine completed normally via Ret.
    Done,
    /// Subroutine hit Suspend — execution paused at pc.
    Suspended { pc: usize },
}

/// External functions callable from the bytecode VM.
///
/// Covers typecheckSysCalls from SysCalls.hs:
///   rename, newSet, insertOutputSet, setToArray, freeSet,
///   newWordSet_, insertBytesWordSet_, wordSetToArray_,
///   byteSetToByteArray_, freeWordSet_
pub trait SysCalls {
    // Bytestring set operations
    fn rename(&mut self, id: Id, pid: Pid) -> Id;
    fn new_set(&mut self) -> BytestringSetHandle;
    fn insert_output_set(&mut self, set: BytestringSetHandle, output: &Output);
    fn set_to_array(&mut self, set: BytestringSetHandle, output: &mut Output);
    fn free_set(&mut self, set: BytestringSetHandle);

    // Word set operations (typecheckSysCalls)
    fn new_word_set(&mut self) -> WordSetHandle;
    fn insert_bytes_word_set(&mut self, set: WordSetHandle, bytes: &[u8], word: u64);
    fn word_set_to_array(&mut self, set: WordSetHandle, output: &mut Output);
    fn byte_set_to_byte_array(&mut self, set: WordSetHandle, output: &mut Output);
    fn free_word_set(&mut self, set: WordSetHandle);

    // Fact lookup
    fn lookup_fact(&self, pid: Pid, key: &[u8]) -> Id;
    fn fact_type(&self, id: Id) -> Pid;
}

/// A no-op SysCalls implementation for testing the VM in isolation.
pub struct NoOpSysCalls;

impl SysCalls for NoOpSysCalls {
    fn rename(&mut self, id: Id, _pid: Pid) -> Id { id }
    fn new_set(&mut self) -> BytestringSetHandle { 0 }
    fn insert_output_set(&mut self, _set: BytestringSetHandle, _output: &Output) {}
    fn set_to_array(&mut self, _set: BytestringSetHandle, _output: &mut Output) {}
    fn free_set(&mut self, _set: BytestringSetHandle) {}
    fn new_word_set(&mut self) -> WordSetHandle { 0 }
    fn insert_bytes_word_set(&mut self, _set: WordSetHandle, _bytes: &[u8], _word: u64) {}
    fn word_set_to_array(&mut self, _set: WordSetHandle, _output: &mut Output) {}
    fn byte_set_to_byte_array(&mut self, _set: WordSetHandle, _output: &mut Output) {}
    fn free_word_set(&mut self, _set: WordSetHandle) {}
    fn lookup_fact(&self, _pid: Pid, _key: &[u8]) -> Id { Id::INVALID }
    fn fact_type(&self, _id: Id) -> Pid { Pid::INVALID }
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
        assert!(out2.is_empty());
        sc.free_set(handle);
    }

    #[test]
    fn test_noop_word_set_ops() {
        let mut sc = NoOpSysCalls;
        let handle = sc.new_word_set();
        assert_eq!(handle, 0);
        sc.insert_bytes_word_set(handle, b"key", 42);
        let mut out = Output::new();
        sc.word_set_to_array(handle, &mut out);
        assert!(out.is_empty());
        let mut out2 = Output::new();
        sc.byte_set_to_byte_array(handle, &mut out2);
        assert!(out2.is_empty());
        sc.free_word_set(handle);
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
        assert_ne!(ExitReason::Done, ExitReason::Suspended { pc: 0 });
    }
}
