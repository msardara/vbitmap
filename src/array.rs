// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`ArrayBitmap`]: an owned bitmap of a fixed number of words, stored inline.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{Deref, DerefMut};

use crate::{BitSlice, Ones, Word};

/// Owned bitmap of exactly `N` words of type `W`, stored inline: no
/// allocation, so it works with the `no_std` feature alone, and it is `Copy`.
///
/// It derefs to [`BitSlice<W>`], so the whole borrowed API applies; writes
/// past `N * W::BITS` bits panic (`set`) or do nothing (`unset`), as on any
/// `BitSlice`. `N` counts words, not bits: `ArrayBitmap<u64, 4>` holds 256
/// bits.
///
/// # Examples
///
/// ```
/// use vbitmap::ArrayBitmap;
///
/// let mut cpus = ArrayBitmap::<u64, 4>::new(); // 256 bits, on the stack
/// cpus.set(3);
/// cpus.set(200);
/// assert_eq!(cpus.iter_ones().collect::<Vec<_>>(), [3, 200]);
/// let copy = cpus; // `Copy`
/// assert_eq!(copy, cpus);
/// ```
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ArrayBitmap<W: Word, const N: usize> {
    words: [W; N],
}

impl<W: Word, const N: usize> ArrayBitmap<W, N> {
    /// All bits clear.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::ArrayBitmap;
    ///
    /// const EMPTY: ArrayBitmap<u32, 2> = ArrayBitmap::new();
    /// assert!(EMPTY.is_zero());
    /// assert_eq!(EMPTY.len_bits(), 64);
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self {
            words: [W::ZERO; N],
        }
    }

    /// From the words, bit 0 being the least significant bit of `words[0]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::ArrayBitmap;
    ///
    /// let b = ArrayBitmap::from_words([0b101u8, 1]);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 2, 8]);
    /// assert_eq!(b.into_words(), [0b101, 1]);
    /// ```
    #[inline]
    pub const fn from_words(words: [W; N]) -> Self {
        Self { words }
    }

    /// The words.
    #[inline]
    pub const fn into_words(self) -> [W; N] {
        self.words
    }
}

impl<W: Word, const N: usize> Default for ArrayBitmap<W, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Word, const N: usize> Deref for ArrayBitmap<W, N> {
    type Target = BitSlice<W>;
    #[inline(always)]
    fn deref(&self) -> &BitSlice<W> {
        BitSlice::new(&self.words)
    }
}

impl<W: Word, const N: usize> DerefMut for ArrayBitmap<W, N> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut BitSlice<W> {
        BitSlice::new_mut(&mut self.words)
    }
}

impl<W: Word, const N: usize> PartialEq for ArrayBitmap<W, N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.words == other.words
    }
}

impl<W: Word, const N: usize> Eq for ArrayBitmap<W, N> {}

/// Hashes like the equal [`BitSlice`], ignoring trailing zero words.
impl<W: Word, const N: usize> Hash for ArrayBitmap<W, N> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<W: Word, const N: usize> fmt::Debug for ArrayBitmap<W, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<W: Word, const N: usize> fmt::Display for ArrayBitmap<W, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<W: Word, const N: usize> From<[W; N]> for ArrayBitmap<W, N> {
    #[inline]
    fn from(words: [W; N]) -> Self {
        Self::from_words(words)
    }
}

/// Sets the bits yielded. Panics at the first index past `N * W::BITS`.
///
/// # Examples
///
/// ```
/// use vbitmap::ArrayBitmap;
///
/// let b: ArrayBitmap<u8, 2> = [1usize, 9].into_iter().collect();
/// assert_eq!(b.into_words(), [0b10, 0b10]);
/// ```
impl<W: Word, const N: usize> FromIterator<usize> for ArrayBitmap<W, N> {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut b = Self::new();
        b.set_many_strict(iter);
        b
    }
}

impl<'a, W: Word, const N: usize> IntoIterator for &'a ArrayBitmap<W, N> {
    type Item = usize;
    type IntoIter = Ones<'a, W>;
    #[inline]
    fn into_iter(self) -> Ones<'a, W> {
        self.iter_ones()
    }
}
