// SPDX-License-Identifier: MIT OR Apache-2.0
use core::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Deref, DerefMut, Not,
};

use alloc::vec::Vec;

use crate::slice::nonzero_len;
use crate::storage::AlignedWords;
use crate::{BitSlice, Word};

/// Growable bitmap, a vector of [`Word`]s.
///
/// All read-only queries are on [`BitSlice`], reachable through `Deref`.
#[derive(Clone, Default)]
pub struct Bitmap<W: Word = usize> {
    pub(crate) words: AlignedWords<W>,
}

/// Equality and hashing ignore trailing zero words, see [`BitSlice`].
impl<W: Word> PartialEq for Bitmap<W> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<W: Word> Eq for Bitmap<W> {}

impl<W: Word> core::hash::Hash for Bitmap<W> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<W: Word> Deref for Bitmap<W> {
    type Target = BitSlice<W>;
    #[inline(always)]
    fn deref(&self) -> &BitSlice<W> {
        BitSlice::new(&self.words)
    }
}

impl<W: Word> DerefMut for Bitmap<W> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut BitSlice<W> {
        BitSlice::new_mut(&mut self.words)
    }
}

impl<W: Word> Bitmap<W> {
    /// Empty bitmap.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let b = NativeBitmap::new();
    /// assert!(b.is_zero());
    /// assert_eq!(b.len_words(), 0);
    /// ```
    #[inline]
    pub const fn new() -> Self {
        Self {
            words: AlignedWords::new(),
        }
    }

    /// Zeroed bitmap whose length covers `n_bits` bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::Bitmap64;
    ///
    /// let b = Bitmap64::alloc(130); // zeroed, covers 130 bits
    /// assert_eq!((b.len_words(), b.len_bits()), (3, 192));
    /// assert!(b.is_zero());
    /// ```
    pub fn alloc(n_bits: usize) -> Self {
        Self {
            words: AlignedWords::zeroed(n_bits.div_ceil(W::BITS)),
        }
    }

    /// Empty bitmap with room for `n_bits` bits before reallocating.
    pub fn with_capacity_bits(n_bits: usize) -> Self {
        Self {
            words: AlignedWords::with_capacity(n_bits.div_ceil(W::BITS)),
        }
    }

    /// Build from existing words, bit 0 being the LSB of `words[0]`. The
    /// words are copied into cache-line aligned storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::Bitmap64;
    ///
    /// let b = Bitmap64::from_words(vec![0b101, 1]); // bit 0 is the LSB of word 0
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 2, 64]);
    /// assert_eq!(b.into_words(), vec![0b101, 1]);
    /// ```
    pub fn from_words(words: Vec<W>) -> Self {
        Self::from_slice(&words)
    }

    /// Like [`Bitmap::from_words`] from a slice.
    pub fn from_slice(words: &[W]) -> Self {
        Self {
            words: AlignedWords::from_slice(words),
        }
    }

    /// The words as a `Vec` (a copy: the storage is cache-line aligned, which
    /// a `Vec` cannot take over).
    pub fn into_words(self) -> Vec<W> {
        self.words.to_vec()
    }

    /// Drop all words, keep the allocation.
    #[inline]
    pub fn clear(&mut self) {
        self.words.clear();
    }

    /// Make sure the bitmap holds at least `n_bits` bits.
    #[inline]
    pub fn validate(&mut self, n_bits: usize) {
        let n = n_bits.div_ceil(W::BITS);
        if n > self.words.len() {
            self.words.resize_zeroed(n);
        }
    }

    /// Grow so that word `i` exists, zero filled. The growth itself is out of
    /// line so the hot paths stay small.
    #[inline(always)]
    pub(crate) fn validate_word(&mut self, i: usize) {
        if i >= self.words.len() {
            self.grow(i);
        }
    }

    #[cold]
    #[inline(never)]
    fn grow(&mut self, i: usize) {
        self.words.resize_zeroed(i + 1);
    }

    /// Would setting bit `i` reallocate?
    #[inline]
    pub fn will_expand(&self, i: usize) -> bool {
        i / W::BITS >= self.words.capacity()
    }

    /// Copy `src` into `self`, reusing the allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let src: NativeBitmap = [1usize, 100].into_iter().collect();
    /// let mut dst = NativeBitmap::new();
    /// dst.copy_from(&src); // reuses `dst`'s allocation when it is large enough
    /// assert_eq!(dst, src);
    /// ```
    #[inline]
    pub fn copy_from(&mut self, src: &BitSlice<W>) {
        self.words.clear();
        self.words.extend_from_slice(&src.words);
    }

    /// Remove trailing zero words.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::alloc(1000);
    /// b.set(3);
    /// assert!(b.len_words() > 1);
    /// b.trim(); // drop the trailing zero words
    /// assert_eq!(b.len_words(), 1);
    /// assert!(b.get(3));
    /// ```
    #[inline]
    pub fn trim(&mut self) {
        // Most bitmaps end in a non-zero word: one load settles it.
        if self.words.last().is_none_or(|&w| w != W::ZERO) {
            return;
        }
        let n = nonzero_len(&self.words);
        self.words.truncate(n);
    }

    /// Set bit `i` to one, growing as needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.set(9);
    /// b.set(300); // grows as needed
    /// b.unset(9);
    /// assert_eq!(b.format_list(), "300");
    /// ```
    #[inline]
    pub fn set(&mut self, i: usize) {
        let i0 = i / W::BITS;
        self.validate_word(i0);
        // SAFETY: validate_word made `i0` a valid index.
        let w = unsafe { self.words.get_unchecked_mut(i0) };
        *w = crate::slice::in_register(*w) | W::bit(i % W::BITS);
    }

    /// Clear bit `i`. Bits past the end are already zero, so nothing happens
    /// there. Never shrinks the bitmap; [`Bitmap::trim`] does.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.unset(1_000_000); // past the end: nothing happens
    /// assert_eq!(b.len_words(), 0);
    ///
    /// b.set(100);
    /// let words = b.len_words();
    /// b.unset(100); // never shrinks...
    /// assert_eq!(b.len_words(), words);
    /// b.trim(); // ...`trim` does
    /// assert_eq!(b.len_words(), 0);
    /// ```
    #[inline]
    pub fn unset(&mut self, i: usize) {
        let i0 = i / W::BITS;
        if let Some(w) = self.words.get_mut(i0) {
            *w = crate::slice::in_register(*w) & !W::bit(i % W::BITS);
        }
    }

    /// Set every bit yielded by `bits`, growing as needed.
    ///
    /// Same result as calling [`Bitmap::set`] for each index, but
    /// faster in a loop: a `&mut Bitmap` forces the compiler to reload the
    /// vector's pointer and length after every store (the store could alias
    /// them), while here the words are borrowed as a slice, so both stay in
    /// registers. Setting bits never needs trimming.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.set_many([3usize, 500, 7, 7]);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [3, 7, 500]);
    /// b.set_many((0..4).map(|i| i * 100));
    /// assert_eq!(b.count_set_bits(), 7);
    /// ```
    pub fn set_many(&mut self, bits: impl IntoIterator<Item = usize>) {
        let mut it = bits.into_iter();
        loop {
            let mut missed = None;
            {
                let w = self.words.as_mut_slice();
                for i in it.by_ref() {
                    match w.get_mut(i / W::BITS) {
                        Some(x) => *x = *x | W::bit(i % W::BITS),
                        None => {
                            missed = Some(i);
                            break;
                        }
                    }
                }
            }
            let Some(i) = missed else { return };
            self.validate_word(i / W::BITS);
            self.words[i / W::BITS] = self.words[i / W::BITS] | W::bit(i % W::BITS);
        }
    }

    /// Store the low `n_bits` of `value` at bit `i`, growing as needed, no
    /// trimming. The field may straddle words, whatever the word type;
    /// panics if `n_bits > 64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.set_field(60, 0b1011, 4); // a 4-bit field at bit 60
    /// assert_eq!(b.get_field(60, 4), 0b1011);
    ///
    /// // Zeros written past the end do not allocate.
    /// let mut c = NativeBitmap::new();
    /// c.set_field(10_000, 0, 8);
    /// assert_eq!(c.len_words(), 0);
    /// ```
    #[inline]
    pub fn set_field(&mut self, i: usize, value: u64, n_bits: usize) {
        if n_bits == 0 {
            return;
        }
        assert!(n_bits <= 64);
        let last = i.checked_add(n_bits - 1).expect("bit index overflow");
        // Writing zeros past the end only needs to clear what exists.
        let mask = if n_bits == 64 {
            !0
        } else {
            (1u64 << n_bits) - 1
        };
        if value & mask == 0 && last >= self.len_bits() {
            if i >= self.len_bits() {
                return;
            }
            let n = self.len_bits() - i;
            return self.set_field_strict(i, 0, n);
        }
        self.validate_word(last / W::BITS);
        self.set_field_strict(i, value, n_bits);
    }

    /// Set or clear `n_bits` bits starting at `i`, growing as needed, no
    /// trimming.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.set_region(10, true, 100); // bits 10 to 109
    /// assert_eq!(b.count_set_bits(), 100);
    /// b.set_region(20, false, 10); // clear bits 20 to 29
    /// assert_eq!(b.count_set_bits(), 90);
    /// assert_eq!((b.first_set(), b.last_set()), (Some(10), Some(109)));
    /// ```
    pub fn set_region(&mut self, i: usize, value: bool, mut n_bits: usize) {
        if n_bits == 0 {
            return;
        }
        let mut end = i.checked_add(n_bits - 1).expect("bit index overflow");
        // Clearing past the end only needs to clear what exists.
        if !value {
            if i >= self.len_bits() {
                return;
            }
            n_bits = n_bits.min(self.len_bits() - i);
            end = i + n_bits - 1;
        }
        let (a0, a1) = (i / W::BITS, i % W::BITS);
        let (b0, b1) = (end / W::BITS, end % W::BITS);
        self.validate_word(b0);

        #[inline(always)]
        fn apply<W: Word>(w: &mut W, mask: W, value: bool) {
            if value {
                *w = *w | mask;
            } else {
                *w = *w & !mask;
            }
        }

        if a0 == b0 {
            apply(&mut self.words[a0], W::low_mask(n_bits) << a1, value);
            return;
        }
        apply(&mut self.words[a0], W::ONES << a1, value);
        // x86-64: both edge words before the middle. glibc's memset ends a
        // short piece with a masked vector store whose 64-byte footprint can
        // cover word `b0`, and a load of `b0` right after it is not forwarded
        // and waits for the store to commit.
        #[cfg(target_arch = "x86_64")]
        apply(&mut self.words[b0], W::ONES >> (W::BITS - b1 - 1), value);
        // `fill` becomes a libc memset call, which costs more than the stores
        // for a few words: write those out directly.
        let f = if value { W::ONES } else { W::ZERO };
        match &mut self.words[a0 + 1..b0] {
            [] => {}
            [a] => *a = f,
            [a, b] => {
                *a = f;
                *b = f;
            }
            [a, b, c] => {
                *a = f;
                *b = f;
                *c = f;
            }
            [a, b, c, d] => {
                *a = f;
                *b = f;
                *c = f;
                *d = f;
            }
            mid => {
                // All zeros or all ones is a byte pattern, so this is a
                // plain memset whatever the word size.
                let byte = if value { 0xFF } else { 0 };
                // SAFETY: `mid` is a valid, exclusively borrowed slice of
                // integers, for which every byte pattern is a valid value.
                unsafe { fill_bytes(mid.as_mut_ptr(), byte, mid.len()) };
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        apply(&mut self.words[b0], W::ONES >> (W::BITS - b1 - 1), value);
    }

    /// `self &= other`, trims.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut a: NativeBitmap = [1usize, 2, 70].into_iter().collect();
    /// let b: NativeBitmap = [2usize, 70, 71].into_iter().collect();
    /// a.and(&b);
    /// assert_eq!(a.iter_ones().collect::<Vec<_>>(), [2, 70]);
    /// ```
    pub fn and(&mut self, other: &BitSlice<W>) {
        let n = self.words.len().min(other.words.len());
        self.words.truncate(n);
        for (a, &b) in self.words.iter_mut().zip(&other.words) {
            *a = *a & b;
        }
        self.trim();
    }

    /// `self &= !other`, trims.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut a: NativeBitmap = [1usize, 2, 70].into_iter().collect();
    /// let b: NativeBitmap = [2usize, 70, 71].into_iter().collect();
    /// a.andnot(&b); // a & !b
    /// assert_eq!(a.iter_ones().collect::<Vec<_>>(), [1]);
    /// ```
    pub fn andnot(&mut self, other: &BitSlice<W>) {
        for (a, &b) in self.words.iter_mut().zip(&other.words) {
            *a = *a & !b;
        }
        self.trim();
    }

    /// `self |= other`, grows, no trim.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut a: NativeBitmap = [1usize, 2].into_iter().collect();
    /// let b: NativeBitmap = [2usize, 300].into_iter().collect();
    /// a.or(&b); // grows to the longer operand
    /// assert_eq!(a.iter_ones().collect::<Vec<_>>(), [1, 2, 300]);
    /// ```
    pub fn or(&mut self, other: &BitSlice<W>) {
        if other.words.len() > self.words.len() {
            self.words.resize_zeroed(other.words.len());
        }
        for (a, &b) in self.words.iter_mut().zip(&other.words) {
            *a = *a | b;
        }
    }

    /// `self ^= other`, grows, trims.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut a: NativeBitmap = [1usize, 2].into_iter().collect();
    /// let b: NativeBitmap = [2usize, 300].into_iter().collect();
    /// a.xor(&b);
    /// assert_eq!(a.iter_ones().collect::<Vec<_>>(), [1, 300]);
    /// ```
    pub fn xor(&mut self, other: &BitSlice<W>) {
        if other.words.len() > self.words.len() {
            self.words.resize_zeroed(other.words.len());
        }
        for (a, &b) in self.words.iter_mut().zip(&other.words) {
            *a = *a ^ b;
        }
        self.trim();
    }

    /// `self & other` as a new bitmap.
    pub fn dup_and(&self, other: &BitSlice<W>) -> Self {
        let n = self.words.len().min(other.words.len());
        let mut r = Self::from_slice(&self.words[..n]);
        for (a, &b) in r.words.iter_mut().zip(&other.words) {
            *a = *a & b;
        }
        r.trim();
        r
    }

    /// `self & !other` as a new bitmap.
    pub fn dup_andnot(&self, other: &BitSlice<W>) -> Self {
        let mut r = self.clone();
        r.andnot(other);
        r
    }

    /// `self | other` as a new bitmap.
    pub fn dup_or(&self, other: &BitSlice<W>) -> Self {
        let (long, short) = if self.words.len() >= other.words.len() {
            (&self.words[..], &other.words[..])
        } else {
            (&other.words[..], &self.words[..])
        };
        let mut r = Self::from_slice(long);
        for (a, &b) in r.words.iter_mut().zip(short) {
            *a = *a | b;
        }
        r
    }

    /// `self ^ other` as a new bitmap.
    pub fn dup_xor(&self, other: &BitSlice<W>) -> Self {
        let (long, short) = if self.words.len() >= other.words.len() {
            (&self.words[..], &other.words[..])
        } else {
            (&other.words[..], &self.words[..])
        };
        let mut r = Self::from_slice(long);
        for (a, &b) in r.words.iter_mut().zip(short) {
            *a = *a ^ b;
        }
        r.trim();
        r
    }

    //
    // `self = a op b` in one pass over memory. Cloning `a` and then applying
    // the in-place operator reads and writes every word twice; these write
    // each result word once. The bitmap's previous contents are discarded.

    /// `self = a & b`, trimmed.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let a: NativeBitmap = [1usize, 2, 70].into_iter().collect();
    /// let b: NativeBitmap = [2usize, 70, 71].into_iter().collect();
    /// let mut dst = NativeBitmap::new();
    /// dst.and_from(&a, &b); // dst = a & b, in one pass; what `dst` held is discarded
    /// assert_eq!(dst.iter_ones().collect::<Vec<_>>(), [2, 70]);
    /// ```
    pub fn and_from(&mut self, a: &BitSlice<W>, b: &BitSlice<W>) {
        let n = a.words.len().min(b.words.len());
        self.words.overwrite(n, |d| {
            for ((d, &x), &y) in d.iter_mut().zip(&a.words).zip(&b.words) {
                *d = x & y;
            }
        });
        self.trim();
    }

    /// `self = a & !b`, trimmed.
    pub fn andnot_from(&mut self, a: &BitSlice<W>, b: &BitSlice<W>) {
        let m = a.words.len().min(b.words.len());
        self.words.overwrite(a.words.len(), |d| {
            let (head, tail) = d.split_at_mut(m);
            for ((d, &x), &y) in head.iter_mut().zip(&a.words).zip(&b.words) {
                *d = x & !y;
            }
            tail.copy_from_slice(&a.words[m..]);
        });
        self.trim();
    }

    /// `self = a | b`, no trim.
    pub fn or_from(&mut self, a: &BitSlice<W>, b: &BitSlice<W>) {
        self.combine_from(a, b, |x, y| x | y);
    }

    /// `self = a ^ b`, trimmed.
    pub fn xor_from(&mut self, a: &BitSlice<W>, b: &BitSlice<W>) {
        self.combine_from(a, b, |x, y| x ^ y);
        self.trim();
    }

    /// Operators whose result is as long as the longer operand: combine the
    /// common words, copy the rest of the longer one.
    #[inline(always)]
    fn combine_from(&mut self, a: &BitSlice<W>, b: &BitSlice<W>, f: impl Fn(W, W) -> W) {
        let (long, short) = if a.words.len() >= b.words.len() {
            (&a.words[..], &b.words[..])
        } else {
            (&b.words[..], &a.words[..])
        };
        self.words.overwrite(long.len(), |d| {
            let (head, tail) = d.split_at_mut(short.len());
            for ((d, &x), &y) in head.iter_mut().zip(long).zip(short) {
                *d = f(x, y);
            }
            // Equal lengths are the common case. On x86-64 the call to memcpy
            // for an empty tail is a fifth of a short bitmap's time, so skip
            // it; on aarch64 the check measured slower at 4096 bits.
            #[cfg(target_arch = "x86_64")]
            if tail.is_empty() {
                return;
            }
            tail.copy_from_slice(&long[short.len()..]);
        });
    }

    #[inline(always)]
    fn imm(&mut self, i: usize, f: impl FnOnce(W, W) -> W) -> W {
        let i0 = i / W::BITS;
        self.validate_word(i0);
        // SAFETY: validate_word made `i0` a valid index.
        let w = unsafe { self.words.get_unchecked_mut(i0) };
        *w = f(*w, W::bit(i % W::BITS));
        *w
    }

    /// Trim after an immediate op left word `i` at zero: only the last word
    /// can create a trailing zero word.
    #[inline(always)]
    fn trim_if_last(&mut self, i: usize) {
        if i / W::BITS + 1 == self.words.len() {
            self.trim();
        }
    }

    /// Word `i / BITS &= 1 << (i % BITS)`, trims. The immediate operators
    /// act on the whole word, not just on bit `i`.
    pub fn andi(&mut self, i: usize) {
        // Past the end the word is zero already: nothing to do.
        if i / W::BITS >= self.words.len() {
            return;
        }
        if self.imm(i, |a, b| a & b) == W::ZERO {
            self.trim_if_last(i);
        }
    }

    /// Word `&= !(1 << i)`, trims.
    pub fn andnoti(&mut self, i: usize) {
        if i / W::BITS >= self.words.len() {
            return;
        }
        if self.imm(i, |a, b| a & !b) == W::ZERO {
            self.trim_if_last(i);
        }
    }

    /// Word `|= 1 << i`.
    pub fn ori(&mut self, i: usize) {
        self.imm(i, |a, b| a | b);
    }

    /// Word `^= 1 << i`, trims.
    pub fn xori(&mut self, i: usize) {
        if self.imm(i, |a, b| a ^ b) == W::ZERO {
            self.trim_if_last(i);
        }
    }

    /// As [`Bitmap::andi`] without trimming.
    pub fn andi_notrim(&mut self, i: usize) {
        self.imm(i, |a, b| a & b);
    }

    /// As [`Bitmap::andnoti`] without trimming.
    pub fn andnoti_notrim(&mut self, i: usize) {
        self.imm(i, |a, b| a & !b);
    }

    /// As [`Bitmap::xori`] without trimming.
    pub fn xori_notrim(&mut self, i: usize) {
        self.imm(i, |a, b| a ^ b);
    }

    /// Random bitmap of `n_bits` bits, bits above `n_bits` are zero.
    /// `rand` yields uniformly distributed 64-bit values.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// // Bring your own generator: any `FnMut() -> u64`.
    /// let mut state = 0x9e37_79b9_7f4a_7c15u64;
    /// let mut rng = move || {
    ///     state ^= state << 13;
    ///     state ^= state >> 7;
    ///     state ^= state << 17;
    ///     state
    /// };
    /// let mut b = NativeBitmap::new();
    /// b.random(100, &mut rng); // bits at or above 100 stay zero
    /// assert!(b.last_set().is_none_or(|p| p < 100));
    /// ```
    pub fn random(&mut self, n_bits: usize, mut rand: impl FnMut() -> u64) {
        self.words.clear();
        if n_bits == 0 {
            return;
        }
        let last = n_bits - 1;
        self.words.resize_zeroed(last / W::BITS + 1);
        // Narrow words are carved out of one 64-bit draw, wide ones take
        // several.
        let (mut buf, mut left) = (0u64, 0usize);
        for w in self.words.iter_mut() {
            if W::BITS >= 64 {
                let mut v = W::from_u64(rand());
                if W::BITS > 64 {
                    v = v | (W::from_u64(rand()) << 64);
                }
                *w = v;
            } else {
                if left == 0 {
                    buf = rand();
                    left = 64;
                }
                *w = W::from_u64(buf);
                buf >>= W::BITS;
                left -= W::BITS;
            }
        }
        let i1 = last % W::BITS;
        if i1 + 1 < W::BITS {
            let t = self.words.last_mut().unwrap();
            *t = *t & W::low_mask(i1 + 1);
        }
        self.trim();
    }
}

impl<W: Word> BitAndAssign<&BitSlice<W>> for Bitmap<W> {
    #[inline]
    fn bitand_assign(&mut self, rhs: &BitSlice<W>) {
        self.and(rhs);
    }
}

impl<W: Word> BitOrAssign<&BitSlice<W>> for Bitmap<W> {
    #[inline]
    fn bitor_assign(&mut self, rhs: &BitSlice<W>) {
        self.or(rhs);
    }
}

impl<W: Word> BitXorAssign<&BitSlice<W>> for Bitmap<W> {
    #[inline]
    fn bitxor_assign(&mut self, rhs: &BitSlice<W>) {
        self.xor(rhs);
    }
}

/// `&a & &b`: a new bitmap, like [`Bitmap::dup_and`].
impl<W: Word> BitAnd<&Bitmap<W>> for &Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitand(self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.dup_and(rhs)
    }
}

/// `&a | &b`: a new bitmap, like [`Bitmap::dup_or`].
impl<W: Word> BitOr<&Bitmap<W>> for &Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitor(self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.dup_or(rhs)
    }
}

/// `&a ^ &b`: a new bitmap, like [`Bitmap::dup_xor`].
impl<W: Word> BitXor<&Bitmap<W>> for &Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitxor(self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.dup_xor(rhs)
    }
}

/// `a & &b`: in place in `a`'s allocation, like [`Bitmap::and`].
impl<W: Word> BitAnd<&Bitmap<W>> for Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitand(mut self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.and(rhs);
        self
    }
}

/// `a | &b`: in place in `a`'s allocation, like [`Bitmap::or`].
impl<W: Word> BitOr<&Bitmap<W>> for Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitor(mut self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.or(rhs);
        self
    }
}

/// `a ^ &b`: in place in `a`'s allocation, like [`Bitmap::xor`].
impl<W: Word> BitXor<&Bitmap<W>> for Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn bitxor(mut self, rhs: &Bitmap<W>) -> Bitmap<W> {
        self.xor(rhs);
        self
    }
}

/// `!a`: the complement within [`len_bits`](BitSlice::len_bits), in place,
/// see [`BitSlice::invert`].
impl<W: Word> Not for Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn not(mut self) -> Bitmap<W> {
        self.invert();
        self
    }
}

/// `!&a`: the complement within [`len_bits`](BitSlice::len_bits), as a new
/// bitmap, see [`BitSlice::invert`].
impl<W: Word> Not for &Bitmap<W> {
    type Output = Bitmap<W>;
    #[inline]
    fn not(self) -> Bitmap<W> {
        let mut r = Bitmap::new();
        r.words.overwrite(self.words.len(), |d| {
            for (d, &s) in d.iter_mut().zip(self.words.iter()) {
                *d = !s;
            }
        });
        r
    }
}

/// Sets the bits yielded, growing as needed, like [`Bitmap::set_many`].
impl<W: Word> Extend<usize> for Bitmap<W> {
    #[inline]
    fn extend<T: IntoIterator<Item = usize>>(&mut self, iter: T) {
        self.set_many(iter);
    }
}

impl<W: Word> FromIterator<usize> for Bitmap<W> {
    fn from_iter<T: IntoIterator<Item = usize>>(iter: T) -> Self {
        let mut bm = Self::new();
        for i in iter {
            bm.set(i);
        }
        bm
    }
}

impl<'a, W: Word> IntoIterator for &'a Bitmap<W> {
    type Item = usize;
    type IntoIter = crate::Ones<'a, W>;
    #[inline]
    fn into_iter(self) -> crate::Ones<'a, W> {
        self.iter_ones()
    }
}

impl<W: Word> From<Vec<W>> for Bitmap<W> {
    #[inline]
    fn from(words: Vec<W>) -> Self {
        Self::from_words(words)
    }
}

/// `ptr::write_bytes(dst, byte, n)`, split at a page boundary on x86-64.
///
/// glibc's memset covers a short range with overlapping vector stores from
/// both ends. The words sit 8 bytes past a cache line, so one of those stores
/// can straddle a 4 KiB page, and a page-split store is slow on recent Intel
/// cores: it doubled `set_region` over 4096 bits. Two calls that each stay
/// inside one page cost less.
///
/// # Safety
/// As for `ptr::write_bytes`.
#[inline(always)]
unsafe fn fill_bytes<W: Word>(dst: *mut W, byte: u8, n: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        const PAGE: usize = 4096;
        let bytes = n * core::mem::size_of::<W>();
        let start = dst as usize;
        let split = (start | (PAGE - 1)) + 1;
        // only short ranges: a long one runs an aligned store loop in which
        // the unaligned ends weigh little
        if bytes <= PAGE && split < start + bytes {
            // `split` is page aligned and `dst` word aligned, so this is exact
            let head = (split - start) / core::mem::size_of::<W>();
            // SAFETY: both pieces lie within the caller's `n` words.
            unsafe {
                core::ptr::write_bytes(dst, byte, head);
                core::ptr::write_bytes(dst.add(head), byte, n - head);
            }
            return;
        }
    }
    // SAFETY: forwarded from the caller.
    unsafe { core::ptr::write_bytes(dst, byte, n) };
}
