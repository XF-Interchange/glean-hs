//! Bytecode opcodes for Glean's fact validation VM.
//!
//! Rust equivalent of glean/bytecode/instruction.h from Meta Glean.
//!
//! 60 active opcodes (0-59), 196 unused slots (60-255).
//! The discriminant fits in a single u8.

/// A single bytecode instruction opcode.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Op {
    // Input operations (0-7)
    InputNat = 0,
    InputByte = 1,
    InputBytes = 2,
    InputSkipUntrustedString = 3,
    InputShiftLit = 4,
    InputShiftBytes = 5,
    InputSkipNat = 6,
    InputSkipTrustedString = 7,

    // Output operations (8-19)
    ResetOutput = 8,
    OutputNat = 9,
    OutputNatImm = 10,
    OutputByte = 11,
    OutputByteImm = 12,
    OutputBytes = 13,
    OutputStringToLower = 14,
    OutputRelToAbsByteSpans = 15,
    OutputUnpackByteSpans = 16,
    OutputStringReverse = 17,
    GetOutput = 18,
    GetOutputSize = 19,

    // Register / load operations (20-28)
    LoadConst = 20,
    LoadLiteral = 21,
    Move = 22,
    SubConst = 23,
    Sub = 24,
    AddConst = 25,
    Add = 26,
    PtrDiff = 27,
    LoadLabel = 28,

    // Control flow (29-41)
    Jump = 29,
    JumpReg = 30,
    JumpIf0 = 31,
    JumpIfNot0 = 32,
    JumpIfEq = 33,
    JumpIfNe = 34,
    JumpIfGt = 35,
    JumpIfGe = 36,
    JumpIfLt = 37,
    JumpIfLe = 38,
    DecrAndJumpIfNot0 = 39,
    DecrAndJumpIf0 = 40,
    Select = 41,

    // System calls (42-54)
    CallFun_0_1 = 42,
    CallFun_0_2 = 43,
    CallFun_1_1 = 44,
    CallFun_1_0 = 45,
    CallFun_2_1 = 46,
    CallFun_2_0 = 47,
    CallFun_3_0 = 48,
    CallFun_4_0 = 49,
    CallFun_3_1 = 50,
    CallFun_5_0 = 51,
    CallFun_5_1 = 52,
    CallFun_2_2 = 53,
    CallFun_2_5 = 54,

    // Debug / lifecycle (55-59)
    Raise = 55,
    Trace = 56,
    TraceReg = 57,
    Suspend = 58,
    Ret = 59,
}

impl Op {
    /// Decode a u8 into an Op.
    /// Returns None for unused opcodes (60-255).
    #[inline]
    pub fn from_u8(b: u8) -> Option<Op> {
        if b <= 59 {
            Some(unsafe { std::mem::transmute(b) })
        } else {
            None
        }
    }

    /// Encode this Op as a u8.
    #[inline]
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Return true if this is a syscall opcode.
    #[inline]
    pub fn is_syscall(self) -> bool {
        let b = self.to_u8();
        b >= 42 && b <= 54
    }

    /// For syscall opcodes, return (n_inputs, n_outputs).
    /// Returns None for non-syscall opcodes.
    pub fn syscall_arity(self) -> Option<(u8, u8)> {
        match self {
            Op::CallFun_0_1 => Some((0, 1)),
            Op::CallFun_0_2 => Some((0, 2)),
            Op::CallFun_1_1 => Some((1, 1)),
            Op::CallFun_1_0 => Some((1, 0)),
            Op::CallFun_2_1 => Some((2, 1)),
            Op::CallFun_2_0 => Some((2, 0)),
            Op::CallFun_3_0 => Some((3, 0)),
            Op::CallFun_4_0 => Some((4, 0)),
            Op::CallFun_3_1 => Some((3, 1)),
            Op::CallFun_5_0 => Some((5, 0)),
            Op::CallFun_5_1 => Some((5, 1)),
            Op::CallFun_2_2 => Some((2, 2)),
            Op::CallFun_2_5 => Some((2, 5)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u8_valid() {
        assert_eq!(Op::from_u8(0),  Some(Op::InputNat));
        assert_eq!(Op::from_u8(29), Some(Op::Jump));
        assert_eq!(Op::from_u8(55), Some(Op::Raise));
        assert_eq!(Op::from_u8(58), Some(Op::Suspend));
        assert_eq!(Op::from_u8(59), Some(Op::Ret));
    }

    #[test]
    fn test_from_u8_invalid() {
        assert_eq!(Op::from_u8(60),  None);
        assert_eq!(Op::from_u8(100), None);
        assert_eq!(Op::from_u8(255), None);
    }

    #[test]
    fn test_roundtrip() {
        for b in 0u8..=59 {
            let op = Op::from_u8(b).unwrap();
            assert_eq!(op.to_u8(), b);
        }
    }

    #[test]
    fn test_syscall_detection() {
        assert!(Op::CallFun_0_1.is_syscall());
        assert!(Op::CallFun_2_5.is_syscall());
        assert!(!Op::Jump.is_syscall());
        assert!(!Op::Ret.is_syscall());
        assert!(!Op::InputNat.is_syscall());
    }

    #[test]
    fn test_syscall_arity() {
        assert_eq!(Op::CallFun_0_1.syscall_arity(), Some((0, 1)));
        assert_eq!(Op::CallFun_2_5.syscall_arity(), Some((2, 5)));
        assert_eq!(Op::CallFun_3_1.syscall_arity(), Some((3, 1)));
        assert_eq!(Op::Jump.syscall_arity(),         None);
        assert_eq!(Op::Ret.syscall_arity(),          None);
    }

    #[test]
    fn test_all_60_opcodes_decodable() {
        let mut count = 0;
        for b in 0u8..=59 {
            if Op::from_u8(b).is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 60);
    }

    #[test]
    fn test_specific_opcodes() {
        assert_eq!(Op::ResetOutput.to_u8(),  8);
        assert_eq!(Op::LoadConst.to_u8(),   20);
        assert_eq!(Op::Jump.to_u8(),        29);
        assert_eq!(Op::CallFun_0_1.to_u8(), 42);
        assert_eq!(Op::Raise.to_u8(),       55);
        assert_eq!(Op::Ret.to_u8(),         59);
    }
}
