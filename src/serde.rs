// SPDX-License-Identifier: MIT OR Apache-2.0
//! `serde` support (the `serde` feature). A bitmap is serialized as its
//! little-endian bytes (bit `i` is bit `i % 8` of byte `i / 8`) without the
//! trailing zero bytes: compact in binary formats, and the same bytes whatever
//! the word type, so a `Bitmap<u8>` reads back what a `Bitmap<u64>` wrote.

use core::fmt;
use core::marker::PhantomData;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ArrayBitmap, BitSlice, Word};

/// Bytes without the trailing zero ones.
fn trimmed(b: &[u8]) -> &[u8] {
    &b[..b.iter().rposition(|&x| x != 0).map_or(0, |p| p + 1)]
}

/// Little-endian bytes, trailing zero bytes dropped. On a big-endian target
/// this needs the `alloc` feature (the bytes are reordered into a buffer).
#[cfg(any(target_endian = "little", has_alloc))]
impl<W: Word> Serialize for BitSlice<W> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[cfg(target_endian = "little")]
        {
            s.serialize_bytes(trimmed(self.as_le_bytes()))
        }
        #[cfg(not(target_endian = "little"))]
        {
            s.serialize_bytes(trimmed(&self.to_bytes()))
        }
    }
}

#[cfg(all(has_alloc, any(target_endian = "little", has_alloc)))]
impl<W: Word> Serialize for crate::Bitmap<W> {
    #[inline]
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(s)
    }
}

#[cfg(any(target_endian = "little", has_alloc))]
impl<W: Word, const N: usize> Serialize for ArrayBitmap<W, N> {
    #[inline]
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(s)
    }
}

/// OR byte `k` (little-endian order) into the words.
#[inline]
fn put_byte<W: Word>(words: &mut [W], k: usize, b: u8) {
    let size = W::BITS / 8;
    let w = &mut words[k / size];
    *w = *w | (W::from_u64(u64::from(b)) << (8 * (k % size)));
}

/// Reads bytes into a growing bitmap.
#[cfg(has_alloc)]
struct BitmapVisitor<W>(PhantomData<W>);

#[cfg(has_alloc)]
impl<'de, W: Word> Visitor<'de> for BitmapVisitor<W> {
    type Value = crate::Bitmap<W>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the little-endian bytes of a bitmap")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        let mut b = crate::Bitmap::from_bytes(v);
        b.trim();
        Ok(b)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut bytes = alloc::vec::Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(x) = seq.next_element::<u8>()? {
            bytes.push(x);
        }
        self.visit_bytes(&bytes)
    }
}

/// Accepts what [`BitSlice`]'s `Serialize` writes: bytes, or a sequence of
/// bytes (as JSON stores them).
#[cfg(has_alloc)]
impl<'de, W: Word> Deserialize<'de> for crate::Bitmap<W> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_bytes(BitmapVisitor(PhantomData))
    }
}

/// Reads bytes into a fixed-size bitmap; a set bit past its size is an error.
struct ArrayVisitor<W, const N: usize>(PhantomData<W>);

impl<W: Word, const N: usize> ArrayVisitor<W, N> {
    /// Store byte `k`, or fail if it holds set bits past the end.
    fn put<E: de::Error>(b: &mut ArrayBitmap<W, N>, k: usize, x: u8) -> Result<(), E> {
        if k < b.bytes() {
            put_byte(b.as_words_mut(), k, x);
        } else if x != 0 {
            return Err(E::invalid_length(
                k + 1,
                &"no set bit past the bitmap's size",
            ));
        }
        Ok(())
    }
}

impl<'de, W: Word, const N: usize> Visitor<'de> for ArrayVisitor<W, N> {
    type Value = ArrayBitmap<W, N>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the little-endian bytes of a bitmap of at most {N} words"
        )
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        let mut b = ArrayBitmap::new();
        for (k, &x) in v.iter().enumerate() {
            Self::put(&mut b, k, x)?;
        }
        Ok(b)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut b = ArrayBitmap::new();
        let mut k = 0;
        while let Some(x) = seq.next_element::<u8>()? {
            Self::put(&mut b, k, x)?;
            k += 1;
        }
        Ok(b)
    }
}

/// Accepts what [`BitSlice`]'s `Serialize` writes; fails if a set bit does
/// not fit in `N` words.
impl<'de, W: Word, const N: usize> Deserialize<'de> for ArrayBitmap<W, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_bytes(ArrayVisitor(PhantomData))
    }
}
