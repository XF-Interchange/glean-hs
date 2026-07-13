//! VM execution frame and register management.
//!
//! Rust equivalent of glean/rts/bytecode/subroutine.h (Activation) from Meta Glean.
//!
//! A Frame holds the register file for one execution of a Subroutine:
//!   registers[0 .. inputs)          input registers (args)
//!   registers[inputs .. inputs+locals) local registers
//!
//! All register values are u64. Typed access (Id, Pid, pointers)
//! is handled by the VM dispatch loop via bit-casting.

use crate::rts::id::{Id, Pid};
use crate::rts::binary::Output;

/// The execution frame for a single subroutine invocation.
/// Holds the register file and output buffers.
pub struct Frame {
    /// All registers: inputs first, then locals.
    pub regs: Vec<u64>,
    /// Output buffers (one per declared output).
    pub outputs: Vec<Output>,
    /// Program counter — index into the subroutine's code vec.
    pub pc: usize,
    /// Number of input registers.
    pub n_inputs: usize,
    /// Number of output registers.
    pub n_outputs: usize,
}

impl Frame {
    /// Create a new Frame for a subroutine with the given register counts.
    pub fn new(n_inputs: usize, n_outputs: usize, n_locals: usize) -> Self {
        let total_regs = n_inputs + n_locals;
        Frame {
            regs:     vec![0u64; total_regs],
            outputs:  (0..n_outputs).map(|_| Output::new()).collect(),
            pc:       0,
            n_inputs,
            n_outputs,
        }
    }

    /// Write an input argument into the frame.
    /// Panics if index >= n_inputs.
    #[inline]
    pub fn set_input(&mut self, index: usize, val: u64) {
        debug_assert!(index < self.n_inputs);
        self.regs[index] = val;
    }

    /// Read a register value.
    #[inline]
    pub fn reg(&self, index: usize) -> u64 {
        self.regs[index]
    }

    /// Write a register value.
    #[inline]
    pub fn set_reg(&mut self, index: usize, val: u64) {
        self.regs[index] = val;
    }

    /// Read a register as an Id.
    #[inline]
    pub fn reg_as_id(&self, index: usize) -> Id {
        Id(self.regs[index])
    }

    /// Read a register as a Pid.
    #[inline]
    pub fn reg_as_pid(&self, index: usize) -> Pid {
        Pid(self.regs[index])
    }

    /// Write an Id into a register.
    #[inline]
    pub fn set_reg_id(&mut self, index: usize, id: Id) {
        self.regs[index] = id.0;
    }

    /// Write a Pid into a register.
    #[inline]
    pub fn set_reg_pid(&mut self, index: usize, pid: Pid) {
        self.regs[index] = pid.0;
    }

    /// Read a register as a raw pointer (for input/output buffer pointers).
    #[inline]
    pub fn reg_as_ptr(&self, index: usize) -> *const u8 {
        self.regs[index] as *const u8
    }

    /// Get a reference to an output buffer.
    #[inline]
    pub fn output(&self, index: usize) -> &Output {
        &self.outputs[index]
    }

    /// Get a mutable reference to an output buffer.
    #[inline]
    pub fn output_mut(&mut self, index: usize) -> &mut Output {
        &mut self.outputs[index]
    }

    /// Reset all output buffers (for reuse).
    pub fn reset_outputs(&mut self) {
        for out in &mut self.outputs {
            out.reset();
        }
    }

    /// Reset the frame for re-execution from a new entry point.
    /// Used by Suspend/resume to restart from a saved pc.
    pub fn restart(&mut self, entry_pc: usize) {
        self.pc = entry_pc;
        self.reset_outputs();
    }

    /// Return the results from all output buffers as byte slices.
    pub fn results(&self) -> Vec<&[u8]> {
        self.outputs.iter().map(|o| o.as_bytes()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_new() {
        let frame = Frame::new(2, 1, 3);
        assert_eq!(frame.regs.len(), 5); // 2 inputs + 3 locals
        assert_eq!(frame.outputs.len(), 1);
        assert_eq!(frame.pc, 0);
        assert_eq!(frame.n_inputs, 2);
        assert_eq!(frame.n_outputs, 1);
    }

    #[test]
    fn test_frame_registers() {
        let mut frame = Frame::new(2, 0, 2);
        frame.set_reg(0, 42);
        frame.set_reg(1, 99);
        assert_eq!(frame.reg(0), 42);
        assert_eq!(frame.reg(1), 99);
    }

    #[test]
    fn test_frame_input() {
        let mut frame = Frame::new(2, 0, 0);
        frame.set_input(0, 100);
        frame.set_input(1, 200);
        assert_eq!(frame.reg(0), 100);
        assert_eq!(frame.reg(1), 200);
    }

    #[test]
    fn test_frame_id_pid() {
        let mut frame = Frame::new(2, 0, 0);
        frame.set_reg_id(0, Id(1024));
        frame.set_reg_pid(1, Pid(7));
        assert_eq!(frame.reg_as_id(0),  Id(1024));
        assert_eq!(frame.reg_as_pid(1), Pid(7));
    }

    #[test]
    fn test_frame_outputs() {
        let mut frame = Frame::new(0, 2, 0);
        frame.output_mut(0).byte(0xAB);
        frame.output_mut(1).byte(0xCD);
        assert_eq!(frame.output(0).as_bytes(), &[0xAB]);
        assert_eq!(frame.output(1).as_bytes(), &[0xCD]);
    }

    #[test]
    fn test_frame_reset_outputs() {
        let mut frame = Frame::new(0, 1, 0);
        frame.output_mut(0).byte(0xFF);
        frame.reset_outputs();
        assert!(frame.output(0).is_empty());
    }

    #[test]
    fn test_frame_restart() {
        let mut frame = Frame::new(0, 1, 0);
        frame.pc = 42;
        frame.output_mut(0).byte(0x01);
        frame.restart(10);
        assert_eq!(frame.pc, 10);
        assert!(frame.output(0).is_empty());
    }

    #[test]
    fn test_frame_results() {
        let mut frame = Frame::new(0, 2, 0);
        frame.output_mut(0).bytes(b"hello");
        frame.output_mut(1).bytes(b"world");
        let results = frame.results();
        assert_eq!(results[0], b"hello");
        assert_eq!(results[1], b"world");
    }

    #[test]
    fn test_frame_all_regs_zero() {
        let frame = Frame::new(3, 0, 4);
        for i in 0..7 {
            assert_eq!(frame.reg(i), 0);
        }
    }
}
