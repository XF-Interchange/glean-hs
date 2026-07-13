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

use crate::rts::nat::{store_nat, load_trusted_nat, MAX_NAT_SIZE, EncodedNat};
use crate::rts::string::{mangle_string, demangle_trusted_string, skip_trusted_string};
use smallvec::SmallVec;

/// Small-buffer capacity matching C++ binary::Output::SMALL_CAP (23 bytes).
/// Values up to 23 bytes are stored inline without heap allocation.
const SMALL_CAP: usize = 23;

/// A write buffer for encoding Glean binary data.
/// Uses SmallVec for small-buffer optimization —
/// avoids heap allocation for small outputs (≤ 23 bytes).
///
/// Equivalent to binary::Output in binary.h.
pub struct Output(SmallVec<[u8; SMALL_CAP]>);

impl Output {
    /// Create a new empty Output buffer.
    #[inline]
    pub fn new() -> Self {
        Output(SmallVec::new())
    }

    /// Return the current contents as a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Return the number of bytes written so far.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return true if no bytes have been written.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Clear the buffer, keeping allocated capacity.
    #[inline]
    pub fn reset(&mut self) {
        self.0.clear();
    }

    /// Consume the Output and return the bytes as a Vec<u8>.
    #[inline]
    pub fn into_vec(self) -> Vec<u8> {
        self.0.into_vec()
    }
}

impl Default for Output {
    fn default() -> Self {
        Output::new()
    }
}

impl Output {
    /// Write a fixed-width value in little-endian byte order.
    /// Used for raw u8, u32, u64 values.
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

    /// Write a nat-encoded u64 (Glean's custom prefix varint).
    #[inline]
    pub fn packed_nat(&mut self, val: u64) {
        let mut buf = [0u8; MAX_NAT_SIZE];
        let n = store_nat(&mut buf, val);
        self.0.extend_from_slice(&buf[..n]);
    }

    /// Write a nat-encoded usize.
    #[inline]
    pub fn packed_usize(&mut self, val: usize) {
        self.packed_nat(val as u64);
    }

    /// Write raw bytes directly (no encoding).
    #[inline]
    pub fn bytes(&mut self, data: &[u8]) {
        self.0.extend_from_slice(data);
    }

    /// Write a single byte.
    #[inline]
    pub fn byte(&mut self, b: u8) {
        self.0.push(b);
    }

    /// Write a mangled UTF-8 string (with NUL escaping and terminator).
    #[inline]
    pub fn mangled_string(&mut self, s: &[u8]) {
        mangle_string(s, &mut self.0);
    }

    /// Write a nat followed by that many bytes (length-prefixed bytes).
    #[inline]
    pub fn nat_prefixed_bytes(&mut self, data: &[u8]) {
        self.packed_nat(data.len() as u64);
        self.bytes(data);
    }
}

/// A read cursor over a borrowed byte slice.
/// Tracks position as it reads through encoded Glean binary data.
///
/// Equivalent to binary::Input in binary.h.
pub struct Input<'a> {
    data: &'a [u8],
}

impl<'a> Input<'a> {
    /// Create a new Input cursor over a byte slice.
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Input { data }
    }

    /// Return the remaining unread bytes.
    #[inline]
    pub fn remaining(&self) -> &[u8] {
        self.data
    }

    /// Return the number of remaining unread bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Return true if no bytes remain.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Read a single byte.
    #[inline]
    pub fn byte(&mut self) -> u8 {
        let b = self.data[0];
        self.data = &self.data[1..];
        b
    }

    /// Read n raw bytes without decoding.
    #[inline]
    pub fn bytes(&mut self, n: usize) -> &'a [u8] {
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        head
    }

    /// Read a fixed-width u32 in little-endian byte order.
    #[inline]
    pub fn fixed_u32(&mut self) -> u32 {
        let bytes = self.bytes(4);
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

    /// Read a fixed-width u64 in little-endian byte order.
    #[inline]
    pub fn fixed_u64(&mut self) -> u64 {
        let bytes = self.bytes(8);
        u64::from_le_bytes(bytes.try_into().unwrap())
    }

    /// Read a nat-encoded u64 (Glean's custom prefix varint).
    #[inline]
    pub fn packed_nat(&mut self) -> u64 {
        let (val, rest) = load_trusted_nat(self.data);
        self.data = rest;
        val
    }

    /// Read a nat-encoded usize.
    #[inline]
    pub fn packed_usize(&mut self) -> usize {
        self.packed_nat() as usize
    }

    /// Read a nat-prefixed byte slice (length then that many bytes).
    #[inline]
    pub fn nat_prefixed_bytes(&mut self) -> &'a [u8] {
        let len = self.packed_usize();
        self.bytes(len)
    }

    /// Skip over a trusted nat without decoding it.
    #[inline]
    pub fn skip_nat(&mut self) {
        use crate::rts::nat::skip_trusted_nat;
        self.data = skip_trusted_nat(self.data);
    }

    /// Read and demangle a trusted UTF-8 string.
    /// Returns the demangled bytes.
    #[inline]
    pub fn trusted_string(&mut self) -> Vec<u8> {
        let (demangled, mangled_size) = demangle_trusted_string(self.data);
        self.data = &self.data[mangled_size..];
        demangled
    }

    /// Skip over a trusted mangled string without decoding it.
    #[inline]
    pub fn skip_trusted_string(&mut self) {
        let (mangled_size, _) = skip_trusted_string(self.data);
        self.data = &self.data[mangled_size..];
    }

    /// Skip over an untrusted mangled string, validating it.
    /// Returns false if the string is invalid or the buffer is too short.
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
        assert_eq!(out.as_bytes(), &[0x78, 0x56, 0x34, 0x12]); // little-endian
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
        assert_eq!(out.as_bytes(), &[0x7F]); // 1-byte nat
        out.reset();
        out.packed_nat(0x80);
        assert_eq!(out.len(), 2); // 2-byte nat
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
        // nat(3) + "abc"
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
        out.byte(0xFF); // sentinel
        let mut input = Input::new(out.as_bytes());
        input.skip_nat();
        assert_eq!(input.byte(), 0xFF);
    }

    #[test]
    fn test_skip_trusted_string() {
        let mut out = Output::new();
        out.mangled_string(b"skip me");
        out.byte(0xAB); // sentinel
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
        let data = [0x00, 0x02]; // invalid escape
        let mut input = Input::new(&data);
        assert!(!input.skip_untrusted_string());
    }

    #[test]
    fn test_small_buffer_optimization() {
        // Values ≤ 23 bytes should not heap allocate
        let mut out = Output::new();
        out.bytes(b"hello world 12345678"); // 20 bytes — fits in SmallVec
        assert_eq!(out.len(), 20);
        assert!(out.len() <= 23);
    }
}
