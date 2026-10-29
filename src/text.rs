// SPDX-License-Identifier: MIT OR Apache-2.0
use core::fmt;

use crate::{BitSlice, Word};

/// Error from `Bitmap::from_hex_mask` and `Bitmap::from_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid bitmap")
    }
}

impl core::error::Error for ParseError {}

impl<W: Word> BitSlice<W> {
    /// Write the bitmap as a hex string, most significant digit first, no
    /// leading zeros. An all-zero or empty bitmap writes `0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0x8efu64];
    /// let mut s = String::new();
    /// BitSlice::new(&words).write_hex(&mut s).unwrap();
    /// assert_eq!(s, "8ef");
    /// ```
    pub fn write_hex<Out: fmt::Write>(&self, out: &mut Out) -> fmt::Result {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut started = false;
        for &w in self.words.iter().rev() {
            for d in (0..W::BITS / 4).rev() {
                let nib = ((w >> (d * 4)) & W::low_mask(4)).to_u64() as usize;
                started |= nib != 0;
                if started {
                    out.write_char(DIGITS[nib] as char)?;
                }
            }
        }
        if !started {
            out.write_char('0')?;
        }
        Ok(())
    }

    /// Write the bitmap as a range list such as `0-3, 5-7, 11`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b0110_0000u64, 1];
    /// let mut s = String::new();
    /// BitSlice::new(&words).write_list(&mut s).unwrap();
    /// assert_eq!(s, "5-6, 64");
    /// ```
    pub fn write_list<Out: fmt::Write>(&self, out: &mut Out) -> fmt::Result {
        let mut cur = self.first_set();
        while let Some(fs) = cur {
            let fc = self.next_clear(fs + 1);
            if fc > fs + 1 {
                write!(out, "{}-{}", fs, fc - 1)?;
            } else {
                write!(out, "{fs}")?;
            }
            cur = self.next_set(fc);
            if cur.is_some() {
                out.write_str(", ")?;
            }
        }
        Ok(())
    }
}

impl<W: Word> fmt::Debug for BitSlice<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BitSlice[")?;
        self.write_list(f)?;
        f.write_str("]")
    }
}

impl<W: Word> fmt::Display for BitSlice<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_list(f)
    }
}

#[cfg(has_alloc)]
mod owned {
    use alloc::string::String;
    use core::fmt;
    use core::str::FromStr;

    use super::ParseError;
    use crate::{BitSlice, Bitmap, Word};

    impl<W: Word> BitSlice<W> {
        /// Hex string, most significant digit first, no leading zeros. An
        /// all-zero or empty bitmap gives `"0"`.
        ///
        /// # Examples
        ///
        /// ```
        /// use vbitmap::NativeBitmap;
        ///
        /// let b = NativeBitmap::from_list("0-3,5-7,11").unwrap();
        /// assert_eq!(b.format_hex(), "8ef");
        /// assert_eq!(NativeBitmap::new().format_hex(), "0");
        /// ```
        pub fn format_hex(&self) -> String {
            let mut s = String::with_capacity(self.words.len() * (W::BITS / 4));
            // writing to a String cannot fail
            let _ = self.write_hex(&mut s);
            s
        }

        /// Range list such as `"0-3, 5-7, 11"`.
        ///
        /// # Examples
        ///
        /// ```
        /// use vbitmap::NativeBitmap;
        ///
        /// let b: NativeBitmap = [0usize, 1, 2, 3, 5, 6, 7, 11].into_iter().collect();
        /// assert_eq!(b.format_list(), "0-3, 5-7, 11");
        /// ```
        pub fn format_list(&self) -> String {
            let mut s = String::new();
            // writing to a String cannot fail
            let _ = self.write_list(&mut s);
            s
        }
    }

    fn parse_u32(s: &str) -> Result<u32, ParseError> {
        let s = s.trim();
        if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
            return Err(ParseError);
        }
        s.parse().map_err(|_| ParseError)
    }

    impl<W: Word> Bitmap<W> {
        /// Parse a hex mask, most significant digit first, optional `0x`
        /// prefix.
        ///
        /// # Examples
        ///
        /// ```
        /// use vbitmap::NativeBitmap;
        ///
        /// let b = NativeBitmap::from_hex_mask("0x8ef").unwrap();
        /// assert_eq!(b.format_list(), "0-3, 5-7, 11");
        /// assert!(NativeBitmap::from_hex_mask("xyz").is_err());
        /// ```
        pub fn from_hex_mask(s: &str) -> Result<Self, ParseError> {
            let s = s.trim();
            let s = s
                .strip_prefix("0x")
                .or_else(|| s.strip_prefix("0X"))
                .unwrap_or(s);
            if s.is_empty() {
                return Err(ParseError);
            }
            // Pre-size so the digits are packed without growing.
            let mut bm = Self::alloc(s.len().checked_mul(4).ok_or(ParseError)?);
            for (n, c) in s.bytes().rev().enumerate() {
                let d = (c as char).to_digit(16).ok_or(ParseError)?;
                bm.set_field_strict(n * 4, d as u64, 4);
            }
            bm.trim();
            Ok(bm)
        }

        /// Parse a range list such as `"0-3,5-7,11"`. A single trailing
        /// comma is accepted.
        ///
        /// # Examples
        ///
        /// ```
        /// use vbitmap::NativeBitmap;
        ///
        /// let b = NativeBitmap::from_list("0-3, 5-7,11").unwrap();
        /// assert_eq!(b.count_set_bits(), 8);
        /// assert!(NativeBitmap::from_list("3-1").is_err()); // reversed range
        /// assert!(NativeBitmap::from_list("").is_err());
        /// ```
        pub fn from_list(s: &str) -> Result<Self, ParseError> {
            let mut bm = Self::new();
            let mut toks = s.split(',').peekable();
            let mut any = false;
            while let Some(tok) = toks.next() {
                let tok = tok.trim();
                if tok.is_empty() && any && toks.peek().is_none() {
                    break;
                }
                let (a, b) = match tok.split_once('-') {
                    Some((a, b)) => (parse_u32(a)?, parse_u32(b)?),
                    None => {
                        let a = parse_u32(tok)?;
                        (a, a)
                    }
                };
                if b < a {
                    return Err(ParseError);
                }
                let n = ((b - a) as usize).checked_add(1).ok_or(ParseError)?;
                bm.set_region(a as usize, true, n);
                any = true;
            }
            // `split` always yields a token, and an empty first one failed to
            // parse above, so at least one range was set here.
            bm.trim();
            Ok(bm)
        }
    }

    impl<W: Word> FromStr for Bitmap<W> {
        type Err = ParseError;
        fn from_str(s: &str) -> Result<Self, ParseError> {
            Self::from_list(s)
        }
    }

    impl<W: Word> fmt::Debug for Bitmap<W> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Bitmap[")?;
            self.write_list(f)?;
            f.write_str("]")
        }
    }

    impl<W: Word> fmt::Display for Bitmap<W> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(&**self, f)
        }
    }
}
