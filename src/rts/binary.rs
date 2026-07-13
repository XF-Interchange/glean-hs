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
