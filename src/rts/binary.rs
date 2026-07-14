//! Binary codec for Glean's fact encoding.
//!
//! Rust equivalent of glean/rts/binary.h from Meta Glean.
//!
//! Two structs:
//!   Output — a write buffer with small-buffer optimization
//!   Input  — a read cursor over a borrowed byte slice
//!
//! Both use Glean's custom nat encoding (nat.rs) and
//! string mangling (string.rs) for their respective operations.

use crate::rts::nat::{store_nat, load_trusted_nat, MAX_NAT_SIZE};
use crate::rts::string::{mangle_string, demangle_trusted_string, skip_trusted_string};
use smallvec::SmallVec;

/// Small-buffer capacity matching C++ binary::Output::SMALL_CAP (23 bytes).
const SMALL_CAP: usize = 23;

/// A write buffer for encoding Glean binary data.
/// Uses SmallVec for small-buffer optimization.
pub struct Output(SmallVec<[u8; SMALL_CAP]>);

impl Output {
    #[inline]
    pub fn new() -> Self {
        Output(SmallVec::new())
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub fn reset(&mut self) {
        self.0.clear();
    }

    #[inline]
    pub fn into_vec(self) -> Vec<u8> {
        self.0.into_vec()
    }

    #[inline]
    pub fn fixed_u8(&mut self, val: u8) {
        self.0.push(val);
    }

    #[inline]
    pub fn fixed_u32(&mut self, val: u32) {
        self.0.extend_from_slice(&val.to_le_bytes());
    }

    #[inline]
    pub fn fixed_u64(&mut self, val: u64) {
        self.0.extend_from_slice(&val.to_le_bytes());
    }

    #[inline]
    pub fn packed_nat(&mut self, val: u64) {
        let mut buf = [0u8; MAX_NAT_SIZE];
        let n = store_nat(&mut buf, val);
        self.0.extend_from_slice(&buf[..n]);
    }

    #[inline]
    pub fn packed_usize(&mut self, val: usize) {
        self.packed_nat(val as u64);
    }

    #[inline]
    pub fn bytes(&mut self, data: &[u8]) {
        self.0.extend_from_slice(data);
    }

    #[inline]
    pub fn byte(&mut self, b: u8) {
        self.0.push(b);
    }

    #[inline]
    pub fn mangled_string(&mut self, s: &[u8]) {
        mangle_string(s, &mut self.0);
    }

    #[inline]
    pub fn nat_prefixed_bytes(&mut self, data: &[u8]) {
        self.packed_nat(data.len() as u64);
        self.bytes(data);
    }
}

impl Default for Output {
    fn default() -> Self {
        Output::new()
    }
}

/// Implement Write so Output can be passed to mangle_string, to_lower_string, etc.
impl std::io::Write for Output {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A read cursor over a borrowed byte slice.
pub struct Input<'a> {
    data: &'a [u8],
}

impl<'a> Input<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Input { data }
    }

    #[inline]
    pub fn remaining(&self) -> &[u8] {
        self.data
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    pub fn byte(&mut self) -> u8 {
        let b = self.data[0];
        self.data = &self.data[1..];
        b
    }

    #[inline]
    pub fn bytes(&mut self, n: usize) -> &'a [u8] {
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        head
    }

    #[inline]
    pub fn fixed_u32(&mut self) -> u32 {
        let bytes = self.bytes(4);
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

    #[inline]
    pub fn fixed_u64(&mut self) -> u64 {
        let bytes = self.bytes(8);
        u64::from_le_bytes(bytes.try_into().unwrap())
    }

    #[inline]
    pub fn packed_nat(&mut self) -> u64 {
        let (val, rest) = load_trusted_nat(self.data);
        self.data = rest;
        val
    }

    #[inline]
    pub fn packed_usize(&mut self) -> usize {
        self.packed_nat() as usize
    }

    #[inline]
    pub fn nat_prefixed_bytes(&mut self) -> &'a [u8] {
        let len = self.packed_usize();
        self.bytes(len)
    }

    #[inline]
    pub fn skip_nat(&mut self) {
        use crate::rts::nat::skip_trusted_nat;
        self.data = skip_trusted_nat(self.data);
    }

    #[inline]
    pub fn trusted_string(&mut self) -> Vec<u8> {
        let (demangled, mangled_size) = demangle_trusted_string(self.data);
        self.data = &self.data[mangled_size..];
        demangled
    }

    #[inline]
    pub fn skip_trusted_string(&mut self) {
        let (mangled_size, _) = skip_trusted_string(self.data);
        self.data = &self.data[mangled_size..];
    }

    pub fn skip_untrusted_string(&mut self) -> bool {
        use crate::rts::string::validate_untrusted_string;
        match validate_untrusted_string(self.data) {
            Some(size) => {
                self.data = &self.data[size..];
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_basic() {
        let mut out = Output::new();
        assert!(out.is_empty());
        out.byte(0x42);
        assert_eq!(out.len(), 1);
        assert_eq!(out.as_bytes(), &[0x42]);
    }

    #[test]
    fn test_output_reset() {
        let mut out = Output::new();
        out.byte(0x01);
        out.reset();
        assert!(out.is_empty());
    }

    #[test]
    fn test_output_fixed_u32() {
        let mut out = Output::new();
        out.fixed_u32(0x12345678);
        assert_eq!(out.as_bytes(), &[0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn test_output_fixed_u64() {
        let mut out = Output::new();
        out.fixed_u64(0x0102030405060708);
        assert_eq!(out.as_bytes(), &[0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn test_output_packed_nat() {
        let mut out = Output::new();
        out.packed_nat(0x7F);
        assert_eq!(out.as_bytes(), &[0x7F]);
        out.reset();
        out.packed_nat(0x80);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_output_mangled_string() {
        let mut out = Output::new();
        out.mangled_string(b"hi");
        assert_eq!(out.as_bytes(), b"hi\x00\x00");
    }

    #[test]
    fn test_output_nat_prefixed_bytes() {
        let mut out = Output::new();
        out.nat_prefixed_bytes(b"abc");
        assert_eq!(out.as_bytes(), &[0x03, b'a', b'b', b'c']);
    }

    #[test]
    fn test_input_byte() {
        let data = [0x01, 0x02, 0x03];
        let mut input = Input::new(&data);
        assert_eq!(input.byte(), 0x01);
        assert_eq!(input.byte(), 0x02);
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_input_fixed_u32() {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut input = Input::new(&data);
        assert_eq!(input.fixed_u32(), 0x12345678);
        assert!(input.is_empty());
    }

    #[test]
    fn test_input_fixed_u64() {
        let data = [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];
        let mut input = Input::new(&data);
        assert_eq!(input.fixed_u64(), 0x0102030405060708);
        assert!(input.is_empty());
    }

    #[test]
    fn test_input_packed_nat() {
        let mut out = Output::new();
        out.packed_nat(12345);
        let mut input = Input::new(out.as_bytes());
        assert_eq!(input.packed_nat(), 12345);
        assert!(input.is_empty());
    }

    #[test]
    fn test_input_trusted_string() {
        let mut out = Output::new();
        out.mangled_string(b"hello");
        let mut input = Input::new(out.as_bytes());
        assert_eq!(input.trusted_string(), b"hello");
        assert!(input.is_empty());
    }

    #[test]
    fn test_input_nat_prefixed_bytes() {
        let mut out = Output::new();
        out.nat_prefixed_bytes(b"abc");
        let mut input = Input::new(out.as_bytes());
        assert_eq!(input.nat_prefixed_bytes(), b"abc");
        assert!(input.is_empty());
    }

    #[test]
    fn test_roundtrip_output_input() {
        let mut out = Output::new();
        out.packed_nat(42);
        out.fixed_u32(0xDEADBEEF);
        out.mangled_string(b"glean");
        out.nat_prefixed_bytes(b"rts");

        let mut input = Input::new(out.as_bytes());
        assert_eq!(input.packed_nat(), 42);
        assert_eq!(input.fixed_u32(), 0xDEADBEEF);
        assert_eq!(input.trusted_string(), b"glean");
        assert_eq!(input.nat_prefixed_bytes(), b"rts");
        assert!(input.is_empty());
    }

    #[test]
    fn test_skip_nat() {
        let mut out = Output::new();
        out.packed_nat(999);
        out.byte(0xFF);
        let mut input = Input::new(out.as_bytes());
        input.skip_nat();
        assert_eq!(input.byte(), 0xFF);
    }

    #[test]
    fn test_skip_trusted_string() {
        let mut out = Output::new();
        out.mangled_string(b"skip me");
        out.byte(0xAB);
        let mut input = Input::new(out.as_bytes());
        input.skip_trusted_string();
        assert_eq!(input.byte(), 0xAB);
    }

    #[test]
    fn test_skip_untrusted_string_valid() {
        let mut out = Output::new();
        out.mangled_string(b"valid");
        let mut input = Input::new(out.as_bytes());
        assert!(input.skip_untrusted_string());
        assert!(input.is_empty());
    }

    #[test]
    fn test_skip_untrusted_string_invalid() {
        let data = [0x00, 0x02];
        let mut input = Input::new(&data);
        assert!(!input.skip_untrusted_string());
    }

    #[test]
    fn test_small_buffer_optimization() {
        let mut out = Output::new();
        out.bytes(b"hello world 12345678");
        assert_eq!(out.len(), 20);
        assert!(out.len() <= 23);
    }
}
