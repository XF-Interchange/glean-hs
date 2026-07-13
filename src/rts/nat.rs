//! Glean's custom prefix varint encoding for natural numbers.
//!
//! Rust equivalent of glean/rts/nat.h from Meta Glean.
//!
//! This is NOT standard LEB128 or protobuf varint.
//! It must be implemented exactly to maintain binary
//! compatibility with existing Glean databases.
//!
//! Key properties:
//!   - Lexicographic ordering: memcmp works for comparison
//!   - Unique representation: every number has exactly one encoding
//!   - Big-endian with range subtraction
//!   - Max 9 bytes for a 64-bit number (vs 10 for standard varint)
//!
//! Encoding table:
//!   First byte    Total bytes    Range
//!   0nnnnnnn           1        0 - 0x7F
//!   10nnnnnn           2        0x80 - 0x407F
//!   110nnnnn           3        0x4080 - 0x20407F
//!   1110nnnn           4        0x204080 - 0x1020407F
//!   11110nnn           5        0x10204080 - 0x081020407F
//!   111110nn           6        0x0810204080 - 0x04081020407F
//!   1111110n           7        0x040810204080 - 0x0204081020407F
//!   11111110           8        0x02040810204080 - 0x010204081020407F
//!   11111111           9        0x010204081020407F - 0xFFFFFFFFFFFFFFFF

/// Maximum number of bytes an encoded nat can occupy.
pub const MAX_NAT_SIZE: usize = 9;

/// Lookup table for nat size from first byte.
/// Faster than computing for common cases (b0 < 0x80 = 1 byte).
static NAT_SIZES: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        t[i] = if i < 0x80      { 1 }
               else if i < 0xC0 { 2 }
               else if i < 0xE0 { 3 }
               else if i < 0xF0 { 4 }
               else if i < 0xF8 { 5 }
               else if i < 0xFC { 6 }
               else if i < 0xFE { 7 }
               else if i < 0xFF { 8 }
               else             { 9 };
        i += 1;
    }
    t
};

/// Compute the total number of bytes used to encode a nat
/// from its first byte. Matches natSize() in nat.h exactly.
#[inline]
pub fn nat_size(b0: u8) -> usize {
    // Fast path for the common case (> 50% of numbers in practice)
    if b0 < 0x80 {
        return 1;
    }
    NAT_SIZES[b0 as usize] as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_size_boundaries() {
        // 1-byte range: 0x00 - 0x7F
        assert_eq!(nat_size(0x00), 1);
        assert_eq!(nat_size(0x7F), 1);
        // 2-byte range: 0x80 - 0xBF
        assert_eq!(nat_size(0x80), 2);
        assert_eq!(nat_size(0xBF), 2);
        // 3-byte range: 0xC0 - 0xDF
        assert_eq!(nat_size(0xC0), 3);
        assert_eq!(nat_size(0xDF), 3);
        // 4-byte range: 0xE0 - 0xEF
        assert_eq!(nat_size(0xE0), 4);
        assert_eq!(nat_size(0xEF), 4);
        // 5-byte range: 0xF0 - 0xF7
        assert_eq!(nat_size(0xF0), 5);
        assert_eq!(nat_size(0xF7), 5);
        // 6-byte range: 0xF8 - 0xFB
        assert_eq!(nat_size(0xF8), 6);
        assert_eq!(nat_size(0xFB), 6);
        // 7-byte range: 0xFC - 0xFD
        assert_eq!(nat_size(0xFC), 7);
        assert_eq!(nat_size(0xFD), 7);
        // 8-byte: 0xFE
        assert_eq!(nat_size(0xFE), 8);
        // 9-byte: 0xFF
        assert_eq!(nat_size(0xFF), 9);
    }
}

/// Range start values for each encoding length.
/// Subtracting these before encoding ensures unique representation
/// and lexicographic ordering.
const RANGE_START: [u64; 9] = [
    0x0000000000000000, // 1 byte:  0
    0x0000000000000080, // 2 bytes: 0x80
    0x0000000000004080, // 3 bytes: 0x4080
    0x0000000000204080, // 4 bytes: 0x204080
    0x0000000010204080, // 5 bytes: 0x10204080
    0x0000000810204080, // 6 bytes: 0x0810204080
    0x0000040810204080, // 7 bytes: 0x040810204080
    0x0002040810204080, // 8 bytes: 0x02040810204080
    0x010204081020407F, // 9 bytes: 0x010204081020407F
];

/// Encode a u64 into Glean's nat format.
/// Returns the number of bytes written.
/// The output buffer must have at least MAX_NAT_SIZE bytes available.
pub fn store_nat(out: &mut [u8], val: u64) -> usize {
    debug_assert!(out.len() >= MAX_NAT_SIZE);

    if val < 0x80 {
        out[0] = val as u8;
        1
    } else if val < 0x4080 {
        let v = val - RANGE_START[1];
        out[0] = 0x80 | (v >> 8) as u8;
        out[1] = (v & 0xFF) as u8;
        2
    } else if val < 0x20_4080 {
        let v = val - RANGE_START[2];
        out[0] = 0xC0 | (v >> 16) as u8;
        out[1] = (v >> 8 & 0xFF) as u8;
        out[2] = (v & 0xFF) as u8;
        3
    } else if val < 0x1020_4080 {
        let v = val - RANGE_START[3];
        out[0] = 0xE0 | (v >> 24) as u8;
        out[1] = (v >> 16 & 0xFF) as u8;
        out[2] = (v >> 8  & 0xFF) as u8;
        out[3] = (v & 0xFF) as u8;
        4
    } else if val < 0x08_1020_4080 {
        let v = val - RANGE_START[4];
        out[0] = 0xF0 | (v >> 32) as u8;
        out[1] = (v >> 24 & 0xFF) as u8;
        out[2] = (v >> 16 & 0xFF) as u8;
        out[3] = (v >> 8  & 0xFF) as u8;
        out[4] = (v & 0xFF) as u8;
        5
    } else if val < 0x0408_1020_4080 {
        let v = val - RANGE_START[5];
        out[0] = 0xF8 | (v >> 40) as u8;
        out[1] = (v >> 32 & 0xFF) as u8;
        out[2] = (v >> 24 & 0xFF) as u8;
        out[3] = (v >> 16 & 0xFF) as u8;
        out[4] = (v >> 8  & 0xFF) as u8;
        out[5] = (v & 0xFF) as u8;
        6
    } else if val < 0x02_0408_1020_4080 {
        let v = val - RANGE_START[6];
        out[0] = 0xFC | (v >> 48) as u8;
        out[1] = (v >> 40 & 0xFF) as u8;
        out[2] = (v >> 32 & 0xFF) as u8;
        out[3] = (v >> 24 & 0xFF) as u8;
        out[4] = (v >> 16 & 0xFF) as u8;
        out[5] = (v >> 8  & 0xFF) as u8;
        out[6] = (v & 0xFF) as u8;
        7
    } else if val < 0x0102_0408_1020_4080 {
        let v = val - RANGE_START[7];
        out[0] = 0xFE;
        out[1] = (v >> 48 & 0xFF) as u8;
        out[2] = (v >> 40 & 0xFF) as u8;
        out[3] = (v >> 32 & 0xFF) as u8;
        out[4] = (v >> 24 & 0xFF) as u8;
        out[5] = (v >> 16 & 0xFF) as u8;
        out[6] = (v >> 8  & 0xFF) as u8;
        out[7] = (v & 0xFF) as u8;
        8
    } else {
        let v = val - RANGE_START[8];
        out[0] = 0xFF;
        out[1] = (v >> 56 & 0xFF) as u8;
        out[2] = (v >> 48 & 0xFF) as u8;
        out[3] = (v >> 40 & 0xFF) as u8;
        out[4] = (v >> 32 & 0xFF) as u8;
        out[5] = (v >> 24 & 0xFF) as u8;
        out[6] = (v >> 16 & 0xFF) as u8;
        out[7] = (v >> 8  & 0xFF) as u8;
        out[8] = (v & 0xFF) as u8;
        9
    }
}

/// A stack-allocated encoded nat. Equivalent to EncodedNat in nat.h.
pub struct EncodedNat {
    buf: [u8; MAX_NAT_SIZE],
    len: usize,
}

impl EncodedNat {
    pub fn new(val: u64) -> Self {
        let mut buf = [0u8; MAX_NAT_SIZE];
        let len = store_nat(&mut buf, val);
        EncodedNat { buf, len }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

    #[test]
    fn test_store_nat_1_byte() {
        let mut buf = [0u8; MAX_NAT_SIZE];
        assert_eq!(store_nat(&mut buf, 0), 1);
        assert_eq!(buf[0], 0x00);
        assert_eq!(store_nat(&mut buf, 0x7F), 1);
        assert_eq!(buf[0], 0x7F);
    }

    #[test]
    fn test_store_nat_2_bytes() {
        let mut buf = [0u8; MAX_NAT_SIZE];
        assert_eq!(store_nat(&mut buf, 0x80), 2);
        assert_eq!(buf[0], 0x80);
        assert_eq!(buf[1], 0x00);
    }

    #[test]
    fn test_store_nat_example_from_spec() {
        // From nat.h: 0x123456 stored as 3 bytes
        // 11010001 11110011 11010110
        let mut buf = [0u8; MAX_NAT_SIZE];
        let n = store_nat(&mut buf, 0x123456);
        assert_eq!(n, 3);
        assert_eq!(buf[0], 0xD1);
        assert_eq!(buf[1], 0xF3);
        assert_eq!(buf[2], 0xD6);
    }

    #[test]
    fn test_store_nat_max() {
        let mut buf = [0u8; MAX_NAT_SIZE];
        assert_eq!(store_nat(&mut buf, u64::MAX), 9);
        assert_eq!(buf[0], 0xFF);
    }

    #[test]
    fn test_encoded_nat() {
        let enc = EncodedNat::new(0x7F);
        assert_eq!(enc.len(), 1);
        assert_eq!(enc.as_bytes(), &[0x7F]);
    }

/// Decode a trusted (already validated) nat from a byte slice.
/// Returns (value, remaining_bytes).
/// Panics in debug mode if the slice is too short.
#[inline]
pub fn load_trusted_nat(p: &[u8]) -> (u64, &[u8]) {
    let size = nat_size(p[0]);
    let val = decode_nat(&p[..size]);
    (val, &p[size..])
}

/// Decode a nat from a byte slice without bounds checking.
/// Internal helper — called only after size is known to be valid.
fn decode_nat(p: &[u8]) -> u64 {
    match p.len() {
        1 => p[0] as u64,
        2 => {
            let v = ((p[0] & 0x3F) as u64) << 8 | p[1] as u64;
            v + RANGE_START[1]
        }
        3 => {
            let v = ((p[0] & 0x1F) as u64) << 16
                  | (p[1] as u64) << 8
                  | p[2] as u64;
            v + RANGE_START[2]
        }
        4 => {
            let v = ((p[0] & 0x0F) as u64) << 24
                  | (p[1] as u64) << 16
                  | (p[2] as u64) << 8
                  | p[3] as u64;
            v + RANGE_START[3]
        }
        5 => {
            let v = ((p[0] & 0x07) as u64) << 32
                  | (p[1] as u64) << 24
                  | (p[2] as u64) << 16
                  | (p[3] as u64) << 8
                  | p[4] as u64;
            v + RANGE_START[4]
        }
        6 => {
            let v = ((p[0] & 0x03) as u64) << 40
                  | (p[1] as u64) << 32
                  | (p[2] as u64) << 24
                  | (p[3] as u64) << 16
                  | (p[4] as u64) << 8
                  | p[5] as u64;
            v + RANGE_START[5]
        }
        7 => {
            let v = ((p[0] & 0x01) as u64) << 48
                  | (p[1] as u64) << 40
                  | (p[2] as u64) << 32
                  | (p[3] as u64) << 24
                  | (p[4] as u64) << 16
                  | (p[5] as u64) << 8
                  | p[6] as u64;
            v + RANGE_START[6]
        }
        8 => {
            let v = (p[1] as u64) << 48
                  | (p[2] as u64) << 40
                  | (p[3] as u64) << 32
                  | (p[4] as u64) << 24
                  | (p[5] as u64) << 16
                  | (p[6] as u64) << 8
                  | p[7] as u64;
            v + RANGE_START[7]
        }
        9 => {
            let v = (p[1] as u64) << 56
                  | (p[2] as u64) << 48
                  | (p[3] as u64) << 40
                  | (p[4] as u64) << 32
                  | (p[5] as u64) << 24
                  | (p[6] as u64) << 16
                  | (p[7] as u64) << 8
                  | p[8] as u64;
            v + RANGE_START[8]
        }
        _ => unreachable!(),
    }
}

/// Decode an untrusted nat, validating the encoding.
/// Returns None if the buffer is too short or the encoding is invalid.
pub fn load_untrusted_nat(p: &[u8]) -> Option<(u64, &[u8])> {
    if p.is_empty() {
        return None;
    }
    let size = nat_size(p[0]);
    if p.len() < size {
        return None;
    }
    // For 9-byte encoding, check for invalid bit patterns
    if size == 9 {
        let val = decode_nat(&p[..9]);
        if val > u64::MAX - RANGE_START[8] {
            return None;
        }
    }
    Some((decode_nat(&p[..size]), &p[size..]))
}

/// Skip over a trusted nat without decoding it.
#[inline]
pub fn skip_trusted_nat(p: &[u8]) -> &[u8] {
    &p[nat_size(p[0])..]
}

    #[test]
    fn test_roundtrip_small() {
        let mut buf = [0u8; MAX_NAT_SIZE];
        for val in [0u64, 1, 42, 0x7F] {
            let n = store_nat(&mut buf, val);
            let (decoded, rest) = load_trusted_nat(&buf[..n]);
            assert_eq!(decoded, val, "roundtrip failed for {}", val);
            assert!(rest.is_empty());
        }
    }

    #[test]
    fn test_roundtrip_all_sizes() {
        let test_vals = [
            0u64,           // 1 byte
            0x80,           // 2 bytes
            0x4080,         // 3 bytes
            0x20_4080,      // 4 bytes
            0x1020_4080,    // 5 bytes
            0x08_1020_4080, // 6 bytes
            0x0408_1020_4080, // 7 bytes
            0x0002_0408_1020_4080, // 8 bytes
            u64::MAX,       // 9 bytes
        ];
        let mut buf = [0u8; MAX_NAT_SIZE];
        for val in test_vals {
            let n = store_nat(&mut buf, val);
            let (decoded, _) = load_trusted_nat(&buf[..n]);
            assert_eq!(decoded, val, "roundtrip failed for 0x{:X}", val);
        }
    }

    #[test]
    fn test_lexicographic_ordering() {
        // Encoded nats must sort the same way as the original numbers
        let vals = [0u64, 1, 0x7F, 0x80, 0x4080, 0x20_4080, u64::MAX];
        let mut buf1 = [0u8; MAX_NAT_SIZE];
        let mut buf2 = [0u8; MAX_NAT_SIZE];
        for i in 0..vals.len() - 1 {
            let n1 = store_nat(&mut buf1, vals[i]);
            let n2 = store_nat(&mut buf2, vals[i + 1]);
            assert!(
                buf1[..n1] < buf2[..n2],
                "lexicographic order failed: {} vs {}",
                vals[i], vals[i+1]
            );
        }
    }

    #[test]
    fn test_untrusted_too_short() {
        assert!(load_untrusted_nat(&[]).is_none());
        // 2-byte encoding but only 1 byte provided
        assert!(load_untrusted_nat(&[0x80]).is_none());
    }

    #[test]
    fn test_skip_trusted_nat() {
        let mut buf = [0u8; MAX_NAT_SIZE + 2];
        let n = store_nat(&mut buf, 0x1234);
        buf[n] = 0xAB; // sentinel byte after the nat
        let rest = skip_trusted_nat(&buf);
        assert_eq!(rest[0], 0xAB);
    }
