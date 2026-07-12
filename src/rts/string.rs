//! UTF-8 string mangling for Glean's binary format.
//!
//! Rust equivalent of glean/rts/string.h from Meta Glean.
//!
//! Glean uses a slightly mangled UTF-8 representation:
//!   - String terminated by: 0x00 0x00
//!   - Embedded NUL (0x00) → 0x00 0x01
//!   - All other bytes: unchanged
//!
//! Properties:
//!   1. Prefix search and compression works correctly
//!   2. memcmp sorts strings as expected
//!   3. Embedded NUL is supported
//!   4. No mangling in the common case (no embedded NUL)

/// Mangle a UTF-8 byte slice into Glean's string format,
/// appending to the output buffer.
pub fn mangle_string(input: &[u8], output: &mut Vec<u8>) {
    for &b in input {
        if b == 0x00 {
            output.push(0x00);
            output.push(0x01);
        } else {
            output.push(b);
        }
    }
    // Write the terminator
    output.push(0x00);
    output.push(0x00);
}

/// Validate an untrusted mangled string.
/// Returns the total mangled size including the terminator,
/// or None if the encoding is invalid or the buffer is too short.
pub fn validate_untrusted_string(input: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < input.len() {
        if input[i] == 0x00 {
            if i + 1 >= input.len() {
                return None; // buffer too short for escape or terminator
            }
            match input[i + 1] {
                0x00 => return Some(i + 2), // terminator found
                0x01 => i += 2,             // escaped NUL, continue
                _    => return None,        // invalid escape sequence
            }
        } else {
            i += 1;
        }
    }
    None // no terminator found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mangle_simple() {
        let mut out = Vec::new();
        mangle_string(b"hello", &mut out);
        assert_eq!(out, b"hello\x00\x00");
    }

    #[test]
    fn test_mangle_empty() {
        let mut out = Vec::new();
        mangle_string(b"", &mut out);
        assert_eq!(out, &[0x00, 0x00]);
    }

    #[test]
    fn test_mangle_embedded_nul() {
        let mut out = Vec::new();
        mangle_string(b"a\x00b", &mut out);
        // 'a' unchanged, NUL → 0x00 0x01, 'b' unchanged, terminator
        assert_eq!(out, &[b'a', 0x00, 0x01, b'b', 0x00, 0x00]);
    }

    #[test]
    fn test_validate_valid() {
        let mut out = Vec::new();
        mangle_string(b"hello", &mut out);
        assert_eq!(validate_untrusted_string(&out), Some(7)); // 5 + terminator
    }

    #[test]
    fn test_validate_empty_string() {
        assert_eq!(validate_untrusted_string(&[0x00, 0x00]), Some(2));
    }

    #[test]
    fn test_validate_too_short() {
        assert_eq!(validate_untrusted_string(&[]), None);
        assert_eq!(validate_untrusted_string(&[b'a']), None); // no terminator
        assert_eq!(validate_untrusted_string(&[0x00]), None); // incomplete escape
    }

    #[test]
    fn test_validate_invalid_escape() {
        // 0x00 followed by anything other than 0x00 or 0x01 is invalid
        assert_eq!(validate_untrusted_string(&[0x00, 0x02, 0x00, 0x00]), None);
    }
}

/// Demangle a trusted mangled string into a byte buffer.
/// Returns (demangled_bytes, mangled_size_including_terminator).
pub fn demangle_trusted_string(input: &[u8]) -> (Vec<u8>, usize) {
    let mut output = Vec::new();
    let mut i = 0;
    loop {
        if input[i] == 0x00 {
            match input[i + 1] {
                0x00 => return (output, i + 2), // terminator
                0x01 => {
                    output.push(0x00); // unescape NUL
                    i += 2;
                }
                _ => unreachable!("invalid mangled string — use validate first"),
            }
        } else {
            output.push(input[i]);
            i += 1;
        }
    }
}

/// Skip over a trusted mangled string without decoding it.
/// Returns (mangled_size, demangled_size) including the terminator.
pub fn skip_trusted_string(input: &[u8]) -> (usize, usize) {
    let mut i = 0;
    let mut demangled = 0;
    loop {
        if input[i] == 0x00 {
            match input[i + 1] {
                0x00 => return (i + 2, demangled), // terminator
                0x01 => {
                    demangled += 1;
                    i += 2;
                }
                _ => unreachable!("invalid mangled string"),
            }
        } else {
            demangled += 1;
            i += 1;
        }
    }
}

/// Convert a mangled string to lowercase in-place,
/// appending the result to output.
/// Only ASCII letters are lowercased — escape sequences
/// and non-ASCII bytes are passed through unchanged.
pub fn to_lower_string(input: &[u8], output: &mut Vec<u8>) {
    let mut i = 0;
    loop {
        if input[i] == 0x00 {
            match input[i + 1] {
                0x00 => {
                    output.push(0x00);
                    output.push(0x00);
                    return;
                }
                0x01 => {
                    // Escaped NUL — pass through unchanged
                    output.push(0x00);
                    output.push(0x01);
                    i += 2;
                }
                _ => unreachable!("invalid mangled string"),
            }
        } else {
            output.push(input[i].to_ascii_lowercase());
            i += 1;
        }
    }
}

    #[test]
    fn test_roundtrip_simple() {
        let mut mangled = Vec::new();
        mangle_string(b"hello world", &mut mangled);
        let (demangled, size) = demangle_trusted_string(&mangled);
        assert_eq!(demangled, b"hello world");
        assert_eq!(size, mangled.len());
    }

    #[test]
    fn test_roundtrip_embedded_nul() {
        let original = b"a\x00b\x00c";
        let mut mangled = Vec::new();
        mangle_string(original, &mut mangled);
        let (demangled, _) = demangle_trusted_string(&mangled);
        assert_eq!(demangled, original);
    }

    #[test]
    fn test_roundtrip_empty() {
        let mut mangled = Vec::new();
        mangle_string(b"", &mut mangled);
        let (demangled, size) = demangle_trusted_string(&mangled);
        assert_eq!(demangled, b"");
        assert_eq!(size, 2);
    }

    #[test]
    fn test_skip_trusted_string() {
        let mut buf = Vec::new();
        mangle_string(b"hello", &mut buf);
        buf.extend_from_slice(b"extra");
        let (mangled_size, demangled_size) = skip_trusted_string(&buf);
        assert_eq!(mangled_size, 7);   // "hello" + terminator
        assert_eq!(demangled_size, 5); // "hello"
        assert_eq!(&buf[mangled_size..], b"extra");
    }

    #[test]
    fn test_to_lower_simple() {
        let mut mangled = Vec::new();
        mangle_string(b"Hello World", &mut mangled);
        let mut result = Vec::new();
        to_lower_string(&mangled, &mut result);
        let (demangled, _) = demangle_trusted_string(&result);
        assert_eq!(demangled, b"hello world");
    }

    #[test]
    fn test_to_lower_preserves_nul_escape() {
        // Embedded NUL should pass through toLower unchanged
        let mut mangled = Vec::new();
        mangle_string(b"A\x00B", &mut mangled);
        let mut result = Vec::new();
        to_lower_string(&mangled, &mut result);
        let (demangled, _) = demangle_trusted_string(&result);
        assert_eq!(demangled, b"a\x00b");
    }

    #[test]
    fn test_memcmp_ordering() {
        // Mangled strings must sort the same as original strings
        let mut a = Vec::new();
        let mut b = Vec::new();
        mangle_string(b"apple", &mut a);
        mangle_string(b"banana", &mut b);
        assert!(a < b, "mangled 'apple' should sort before mangled 'banana'");
    }
