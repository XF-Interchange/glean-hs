//! Bytecode VM execution loop.
//!
//! Rust equivalent of glean/bytecode/evaluate.h from Meta Glean.
//!
//! The VM executes a Subroutine against a Frame, dispatching on
//! each opcode in sequence. Execution continues until Ret or Suspend.

use crate::rts::nat::{load_trusted_nat, store_nat, skip_trusted_nat, MAX_NAT_SIZE};
use crate::rts::string::{
    mangle_string, demangle_trusted_string,
    skip_trusted_string, to_lower_string,
    validate_untrusted_string,
};
use crate::rts::binary::Output;
use crate::rts::bytecode::opcode::Op;
use crate::rts::bytecode::frame::Frame;
use crate::rts::bytecode::syscall::{SysCalls, ExitReason};

/// A compiled subroutine — an immutable bytecode program.
pub struct Subroutine {
    /// The instruction stream. Each u64 word contains:
    ///   bits 0-7:   opcode (Op as u8)
    ///   bits 8-63:  packed operands (opcode-dependent)
    pub code:      Vec<u64>,
    /// Number of input registers.
    pub n_inputs:  usize,
    /// Number of output buffers.
    pub n_outputs: usize,
    /// Number of local registers (beyond inputs).
    pub n_locals:  usize,
    /// Constant pool — values loaded by LoadConst.
    pub constants: Vec<u64>,
    /// String literal pool — loaded by LoadLiteral.
    pub literals:  Vec<Vec<u8>>,
}

impl Subroutine {
    /// Create a new Subroutine.
    pub fn new(
        code:      Vec<u64>,
        n_inputs:  usize,
        n_outputs: usize,
        n_locals:  usize,
        constants: Vec<u64>,
        literals:  Vec<Vec<u8>>,
    ) -> Self {
        Subroutine { code, n_inputs, n_outputs, n_locals, constants, literals }
    }

    /// Create a new Frame for executing this subroutine.
    pub fn new_frame(&self) -> Frame {
        Frame::new(self.n_inputs, self.n_outputs, self.n_locals)
    }

    /// Execute this subroutine against a frame.
    ///
    /// The frame's input registers must be set before calling.
    /// Returns ExitReason::Done or ExitReason::Suspended.
    ///
    /// On Suspend, the frame retains its state — call execute()
    /// again with the same frame to resume.
    pub fn execute<S: SysCalls>(
        &self,
        frame: &mut Frame,
        syscalls: &mut S,
    ) -> Result<ExitReason, VmError> {
        loop {
            if frame.pc >= self.code.len() {
                return Err(VmError::PcOutOfBounds(frame.pc));
            }

            let word = self.code[frame.pc];
            let op_byte = (word & 0xFF) as u8;
            let op = Op::from_u8(op_byte)
                .ok_or(VmError::InvalidOpcode(op_byte))?;

            // Operands are packed into bits 8+ of the word
            // reg(n) extracts the nth 8-bit register index
            let reg = |n: u32| -> usize {
                ((word >> (8 + n * 8)) & 0xFF) as usize
            };
            let imm16 = || -> u64 { (word >> 16) & 0xFFFF };
            let imm32 = || -> u64 { (word >> 32) & 0xFFFFFFFF };

            match op {
                // ── Input operations ──────────────────────────────────
                Op::InputNat => {
                    let dst  = reg(0);
                    let src  = reg(1); // register holding input ptr
                    let ptr  = frame.reg(src) as *const u8;
                    // SAFETY: ptr must point to valid Glean-encoded data
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, MAX_NAT_SIZE * 2)
                    };
                    let (val, rest) = load_trusted_nat(slice);
                    let consumed = rest.as_ptr() as usize - ptr as usize;
                    frame.set_reg(dst, val);
                    frame.set_reg(src, frame.reg(src) + consumed as u64);
                    frame.pc += 1;
                }

                Op::InputByte => {
                    let dst = reg(0);
                    let src = reg(1);
                    let ptr = frame.reg(src) as *const u8;
                    let b = unsafe { *ptr };
                    frame.set_reg(dst, b as u64);
                    frame.set_reg(src, frame.reg(src) + 1);
                    frame.pc += 1;
                }

                Op::InputSkipNat => {
                    let src = reg(0);
                    let ptr = frame.reg(src) as *const u8;
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, MAX_NAT_SIZE * 2)
                    };
                    let rest = skip_trusted_nat(slice);
                    let consumed = rest.as_ptr() as usize - ptr as usize;
                    frame.set_reg(src, frame.reg(src) + consumed as u64);
                    frame.pc += 1;
                }

                Op::InputSkipTrustedString => {
                    let src = reg(0);
                    let ptr = frame.reg(src) as *const u8;
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, 65536)
                    };
                    let (mangled_size, _) = skip_trusted_string(slice);
                    frame.set_reg(src, frame.reg(src) + mangled_size as u64);
                    frame.pc += 1;
                }

                Op::InputSkipUntrustedString => {
                    let src = reg(0);
                    let ptr = frame.reg(src) as *const u8;
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, 65536)
                    };
                    match validate_untrusted_string(slice) {
                        Some(size) => {
                            frame.set_reg(src, frame.reg(src) + size as u64);
                            frame.pc += 1;
                        }
                        None => return Err(VmError::InvalidString(frame.pc)),
                    }
                }

                Op::InputBytes => {
                    let dst   = reg(0); // dest register for bytes ptr
                    let src   = reg(1); // input ptr register
                    let n_reg = reg(2); // register holding byte count
                    let n     = frame.reg(n_reg) as usize;
                    let ptr   = frame.reg(src);
                    frame.set_reg(dst, ptr);   // store start ptr in dst
                    frame.set_reg(src, ptr + n as u64);
                    frame.pc += 1;
                }

                Op::InputShiftLit => {
                    let src = reg(0);
                    let n   = imm32();
                    frame.set_reg(src, frame.reg(src) + n);
                    frame.pc += 1;
                }

                Op::InputShiftBytes => {
                    let src   = reg(0);
                    let n_reg = reg(1);
                    let n     = frame.reg(n_reg);
                    frame.set_reg(src, frame.reg(src) + n);
                    frame.pc += 1;
                }

                // ── Output operations ─────────────────────────────────
                Op::ResetOutput => {
                    let out_idx = reg(0);
                    frame.output_mut(out_idx).reset();
                    frame.pc += 1;
                }

                Op::OutputNat => {
                    let out_idx = reg(0);
                    let val_reg = reg(1);
                    let val     = frame.reg(val_reg);
                    frame.output_mut(out_idx).packed_nat(val);
                    frame.pc += 1;
                }

                Op::OutputNatImm => {
                    let out_idx = reg(0);
                    let val     = imm32();
                    frame.output_mut(out_idx).packed_nat(val);
                    frame.pc += 1;
                }

                Op::OutputByte => {
                    let out_idx = reg(0);
                    let val_reg = reg(1);
                    let b       = frame.reg(val_reg) as u8;
                    frame.output_mut(out_idx).byte(b);
                    frame.pc += 1;
                }

                Op::OutputByteImm => {
                    let out_idx = reg(0);
                    let b       = (word >> 16) as u8;
                    frame.output_mut(out_idx).byte(b);
                    frame.pc += 1;
                }

                Op::OutputBytes => {
                    let out_idx = reg(0);
                    let ptr_reg = reg(1);
                    let len_reg = reg(2);
                    let ptr = frame.reg(ptr_reg) as *const u8;
                    let len = frame.reg(len_reg) as usize;
                    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
                    frame.output_mut(out_idx).bytes(data);
                    frame.pc += 1;
                }

                Op::OutputStringToLower => {
                    let out_idx = reg(0);
                    let ptr_reg = reg(1);
                    let ptr = frame.reg(ptr_reg) as *const u8;
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, 65536)
                    };
                    let out = frame.output_mut(out_idx);
                    to_lower_string(slice, out);
                    frame.pc += 1;
                }

                Op::OutputStringReverse => {
                    let out_idx = reg(0);
                    let ptr_reg = reg(1);
                    let ptr = frame.reg(ptr_reg) as *const u8;
                    let slice = unsafe {
                        std::slice::from_raw_parts(ptr, 65536)
                    };
                    let (demangled, _) = demangle_trusted_string(slice);
                    let mut reversed = demangled;
                    reversed.reverse();
                    let out = frame.output_mut(out_idx);
                    mangle_string(&reversed, out);
                    frame.pc += 1;
                }

                Op::GetOutput => {
                    let dst_reg = reg(0);
                    let out_idx = reg(1);
                    let ptr = frame.output(out_idx).as_bytes().as_ptr() as u64;
                    frame.set_reg(dst_reg, ptr);
                    frame.pc += 1;
                }

                Op::GetOutputSize => {
                    let dst_reg = reg(0);
                    let out_idx = reg(1);
                    let size = frame.output(out_idx).len() as u64;
                    frame.set_reg(dst_reg, size);
                    frame.pc += 1;
                }

                Op::OutputRelToAbsByteSpans | Op::OutputUnpackByteSpans => {
                    // Stub — complex span operations, implement in Phase 11
                    frame.pc += 1;
                }

                // ── Register / load operations ────────────────────────
                Op::LoadConst => {
                    let dst = reg(0);
                    let idx = imm32() as usize;
                    let val = self.constants.get(idx)
                        .copied()
                        .ok_or(VmError::ConstantOutOfBounds(idx))?;
                    frame.set_reg(dst, val);
                    frame.pc += 1;
                }

                Op::LoadLiteral => {
                    let dst = reg(0);
                    let idx = imm32() as usize;
                    let lit = self.literals.get(idx)
                        .ok_or(VmError::LiteralOutOfBounds(idx))?;
                    frame.set_reg(dst, lit.as_ptr() as u64);
                    frame.pc += 1;
                }

                Op::Move => {
                    let dst = reg(0);
                    let src = reg(1);
                    frame.set_reg(dst, frame.reg(src));
                    frame.pc += 1;
                }

                Op::AddConst => {
                    let dst = reg(0);
                    let val = imm32();
                    frame.set_reg(dst, frame.reg(dst).wrapping_add(val));
                    frame.pc += 1;
                }

                Op::Add => {
                    let dst = reg(0);
                    let src = reg(1);
                    let result = frame.reg(dst).wrapping_add(frame.reg(src));
                    frame.set_reg(dst, result);
                    frame.pc += 1;
                }

                Op::SubConst => {
                    let dst = reg(0);
                    let val = imm32();
                    frame.set_reg(dst, frame.reg(dst).wrapping_sub(val));
                    frame.pc += 1;
                }

                Op::Sub => {
                    let dst = reg(0);
                    let src = reg(1);
                    let result = frame.reg(dst).wrapping_sub(frame.reg(src));
                    frame.set_reg(dst, result);
                    frame.pc += 1;
                }

                Op::PtrDiff => {
                    let dst = reg(0);
                    let src = reg(1);
                    let diff = frame.reg(dst).wrapping_sub(frame.reg(src));
                    frame.set_reg(dst, diff);
                    frame.pc += 1;
                }

                Op::LoadLabel => {
                    let dst    = reg(0);
                    let target = imm32() as u64;
                    frame.set_reg(dst, target);
                    frame.pc += 1;
                }

                // ── Control flow ──────────────────────────────────────
                Op::Jump => {
                    let target = imm32() as usize;
                    frame.pc = target;
                }

                Op::JumpReg => {
                    let addr_reg = reg(0);
                    frame.pc = frame.reg(addr_reg) as usize;
                }

                Op::JumpIf0 => {
                    let cond   = reg(0);
                    let target = imm32() as usize;
                    if frame.reg(cond) == 0 {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfNot0 => {
                    let cond   = reg(0);
                    let target = imm32() as usize;
                    if frame.reg(cond) != 0 {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfEq => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) == frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfNe => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) != frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfGt => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) > frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfGe => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) >= frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfLt => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) < frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::JumpIfLe => {
                    let a      = reg(0);
                    let b      = reg(1);
                    let target = imm16() as usize;
                    if frame.reg(a) <= frame.reg(b) {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::DecrAndJumpIfNot0 => {
                    let reg_idx = reg(0);
                    let target  = imm32() as usize;
                    let val     = frame.reg(reg_idx).wrapping_sub(1);
                    frame.set_reg(reg_idx, val);
                    if val != 0 {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::DecrAndJumpIf0 => {
                    let reg_idx = reg(0);
                    let target  = imm32() as usize;
                    let val     = frame.reg(reg_idx).wrapping_sub(1);
                    frame.set_reg(reg_idx, val);
                    if val == 0 {
                        frame.pc = target;
                    } else {
                        frame.pc += 1;
                    }
                }

                Op::Select => {
                    // Jump table: reg(0) is index, following words are targets
                    let idx    = frame.reg(reg(0)) as usize;
                    let n_arms = imm32() as usize;
                    if idx < n_arms {
                        let target_word = self.code.get(frame.pc + 1 + idx)
                            .ok_or(VmError::PcOutOfBounds(frame.pc + 1 + idx))?;
                        frame.pc = *target_word as usize;
                    } else {
                        frame.pc += 1 + n_arms; // skip table
                    }
                }

                // ── System calls ──────────────────────────────────────
                Op::CallFun_1_0 => {
                    let a = frame.reg(reg(0));
                    let _ = a; // consume — actual syscall determined by imm
                    frame.pc += 1;
                }

                Op::CallFun_1_1 => {
                    let src = reg(0);
                    let dst = reg(1);
                    // Rename is the primary 1→1 syscall
                    let id  = crate::rts::id::Id(frame.reg(src));
                    let pid = crate::rts::id::Pid(0); // default
                    let new_id = syscalls.rename(id, pid);
                    frame.set_reg(dst, new_id.0);
                    frame.pc += 1;
                }

                Op::CallFun_0_1 => {
                    let dst    = reg(0);
                    let handle = syscalls.new_set();
                    frame.set_reg(dst, handle);
                    frame.pc += 1;
                }

                Op::CallFun_2_0 | Op::CallFun_2_1 | Op::CallFun_2_2
                | Op::CallFun_2_5 | Op::CallFun_3_0 | Op::CallFun_3_1
                | Op::CallFun_4_0 | Op::CallFun_5_0 | Op::CallFun_5_1
                | Op::CallFun_0_2 => {
                    // Stub remaining syscalls — implement in Phase 11
                    // when connected to the Haskell FFI layer
                    frame.pc += 1;
                }

                // ── Debug / lifecycle ─────────────────────────────────
                Op::Raise => {
                    let idx = imm32() as usize;
                    let msg = self.literals.get(idx)
                        .map(|v| String::from_utf8_lossy(v).into_owned())
                        .unwrap_or_else(|| format!("literal #{}", idx));
                    return Err(VmError::Raised(msg));
                }

                Op::Trace | Op::TraceReg => {
                    // Debug tracing — noop in production
                    frame.pc += 1;
                }

                Op::Suspend => {
                    frame.pc += 1; // advance past Suspend for resumption
                    return Ok(ExitReason::Suspended { pc: frame.pc });
                }

                Op::Ret => {
                    return Ok(ExitReason::Done);
                }
            }
        }
    }
}

/// Errors that can occur during VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// Program counter went out of bounds.
    PcOutOfBounds(usize),
    /// Unknown opcode encountered.
    InvalidOpcode(u8),
    /// Constant pool index out of bounds.
    ConstantOutOfBounds(usize),
    /// Literal pool index out of bounds.
    LiteralOutOfBounds(usize),
    /// Raise opcode executed with message.
    Raised(String),
    /// Invalid string encoding encountered.
    InvalidString(usize),
}

/// Helper to encode a single instruction word.
/// opcode in bits 0-7, operands packed into higher bits.
pub fn encode_instr(op: Op, r0: u8, r1: u8, r2: u8, imm: u32) -> u64 {
    (op.to_u8() as u64)
        | ((r0 as u64) << 8)
        | ((r1 as u64) << 16)
        | ((r2 as u64) << 24)
        | ((imm as u64) << 32)
}

/// Helper to encode an instruction with a 32-bit immediate.
/// The immediate is placed at bit 32 to match the VM's imm32() reader.
pub fn encode_instr_imm(op: Op, r0: u8, imm: u32) -> u64 {
    (op.to_u8() as u64)
        | ((r0 as u64) << 8)
        | ((imm as u64) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rts::bytecode::syscall::NoOpSysCalls;

    fn make_sub(code: Vec<u64>) -> Subroutine {
        Subroutine::new(code, 0, 0, 4, vec![], vec![])
    }

    fn make_sub_with_consts(code: Vec<u64>, constants: Vec<u64>) -> Subroutine {
        Subroutine::new(code, 0, 1, 4, constants, vec![])
    }

    #[test]
    fn test_ret() {
        let sub = make_sub(vec![Op::Ret.to_u8() as u64]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        assert_eq!(sub.execute(&mut frame, &mut sc), Ok(ExitReason::Done));
    }

    #[test]
    fn test_load_const() {
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0), // load constants[0] into reg[0]
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![42]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(0), 42);
    }

    #[test]
    fn test_move() {
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0), // reg[0] = 99
            encode_instr(Op::Move, 1, 0, 0, 0),    // reg[1] = reg[0]
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![99]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(1), 99);
    }

    #[test]
    fn test_add_const() {
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0),  // reg[0] = 10
            encode_instr_imm(Op::AddConst, 0, 5),   // reg[0] += 5
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![10]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(0), 15);
    }

    #[test]
    fn test_jump() {
        let code = vec![
            encode_instr_imm(Op::Jump, 0, 2),       // jump to pc=2
            encode_instr_imm(Op::LoadConst, 0, 0),  // skipped — reg[0] = 999
            Op::Ret.to_u8() as u64,                 // pc=2: ret
        ];
        let sub = make_sub_with_consts(code, vec![999]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(0), 0); // LoadConst was skipped
    }

    #[test]
    fn test_jump_if0_taken() {
        // reg[0] starts at 0, so JumpIf0 should jump
        let code = vec![
            encode_instr_imm(Op::JumpIf0, 0, 2),   // if reg[0]==0, jump to 2
            encode_instr_imm(Op::LoadConst, 1, 0),  // skipped
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![999]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(1), 0); // LoadConst was skipped
    }

    #[test]
    fn test_jump_if0_not_taken() {
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0),  // reg[0] = 1
            encode_instr_imm(Op::JumpIf0, 0, 3),    // reg[0]!=0, don't jump
            encode_instr_imm(Op::LoadConst, 1, 1),  // reg[1] = 42
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![1, 42]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(1), 42);
    }

    #[test]
    fn test_suspend_resume() {
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0), // reg[0] = 7
            Op::Suspend.to_u8() as u64,
            encode_instr_imm(Op::LoadConst, 1, 1), // reg[1] = 8 (after resume)
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![7, 8]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;

        // First execution — hits Suspend
        let result = sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(result, ExitReason::Suspended { pc: 2 });
        assert_eq!(frame.reg(0), 7);
        assert_eq!(frame.reg(1), 0); // not yet set

        // Resume from where we left off
        let result2 = sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(result2, ExitReason::Done);
        assert_eq!(frame.reg(1), 8); // now set
    }

    #[test]
    fn test_output_nat_imm() {
        let code = vec![
            encode_instr_imm(Op::OutputNatImm, 0, 42), // output[0] ← nat(42)
            Op::Ret.to_u8() as u64,
        ];
        let sub = Subroutine::new(code, 0, 1, 0, vec![], vec![]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        // nat(42) = 0x2A (single byte, < 0x80)
        assert_eq!(frame.output(0).as_bytes(), &[0x2A]);
    }

    #[test]
    fn test_raise() {
        let code = vec![
            encode_instr_imm(Op::Raise, 0, 0), // raise literals[0]
        ];
        let sub = Subroutine::new(
            code, 0, 0, 0, vec![],
            vec![b"test error".to_vec()],
        );
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        let result = sub.execute(&mut frame, &mut sc);
        assert!(matches!(result, Err(VmError::Raised(_))));
    }

    #[test]
    fn test_decr_and_jump() {
        // Count down from 3 to 0 using DecrAndJumpIfNot0
        // reg[0] = 3, loop back while != 0
        let code = vec![
            encode_instr_imm(Op::LoadConst, 0, 0),      // reg[0] = 3
            encode_instr_imm(Op::DecrAndJumpIfNot0, 0, 1), // decr reg[0], jump to 1 if !=0
            Op::Ret.to_u8() as u64,
        ];
        let sub = make_sub_with_consts(code, vec![3]);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        sub.execute(&mut frame, &mut sc).unwrap();
        assert_eq!(frame.reg(0), 0); // counted down to 0
    }

    #[test]
    fn test_invalid_opcode() {
        let code = vec![200u64]; // opcode 200 — invalid
        let sub = make_sub(code);
        let mut frame = sub.new_frame();
        let mut sc = NoOpSysCalls;
        assert!(matches!(
            sub.execute(&mut frame, &mut sc),
            Err(VmError::InvalidOpcode(200))
        ));
    }
}
