// SPDX-License-Identifier: MIT OR Apache-2.0
//! Set algebra between two bitmaps that builds no result: subset and
//! disjointness tests, population counts of `a op b`, and lazy iterators over
//! the set bits of `a op b`.
//!
//! Two bitmaps of different lengths are compared as if the shorter one were
//! padded with zero words, like everywhere else in the crate.

use core::iter::FusedIterator;
use core::marker::PhantomData;

use crate::iter::decode_word;
use crate::{BitSlice, Word};

/// A bitwise operator and what it makes of the words that only one operand
/// has (the other reads as zero there).
pub(crate) trait Op: Copy + Default {
    /// `a op b`.
    fn word<W: Word>(a: W, b: W) -> W;
    /// `a op 0 == a`: the words only `a` has pass through.
    const KEEP_A: bool;
    /// `0 op b == b`: the words only `b` has pass through.
    const KEEP_B: bool;
    /// The same operator on 16 bytes.
    #[cfg(target_arch = "aarch64")]
    fn neon(
        a: core::arch::aarch64::uint8x16_t,
        b: core::arch::aarch64::uint8x16_t,
    ) -> core::arch::aarch64::uint8x16_t;
}

macro_rules! op {
    ($(#[$doc:meta])* $name:ident, $a:ident, $b:ident, $word:expr, $keep_a:expr, $keep_b:expr, $neon:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default)]
        pub(crate) struct $name;
        impl Op for $name {
            #[inline(always)]
            fn word<W: Word>($a: W, $b: W) -> W {
                $word
            }
            const KEEP_A: bool = $keep_a;
            const KEEP_B: bool = $keep_b;
            #[cfg(target_arch = "aarch64")]
            #[inline(always)]
            fn neon(
                $a: core::arch::aarch64::uint8x16_t,
                $b: core::arch::aarch64::uint8x16_t,
            ) -> core::arch::aarch64::uint8x16_t {
                // SAFETY: NEON is part of the aarch64 baseline.
                unsafe { core::arch::aarch64::$neon($a, $b) }
            }
        }
    };
}

op!(
    /// `a & b`, see [`BitSlice::iter_and`].
    And, a, b, a & b, false, false, vandq_u8
);
op!(
    /// `a | b`, see [`BitSlice::iter_or`].
    Or, a, b, a | b, true, true, vorrq_u8
);
op!(
    /// `a ^ b`, see [`BitSlice::iter_xor`].
    Xor, a, b, a ^ b, true, true, veorq_u8
);
op!(
    /// `a & !b`, see [`BitSlice::iter_andnot`].
    AndNot, a, b, a & !b, true, false, vbicq_u8
);

/// Is `a op b` non-zero in any word both operands have? Eight words per test
/// with no branch in between, so the reduction vectorizes and the early exit
/// is checked once per cache line.
#[inline]
fn any_common<W: Word, O: Op>(a: &[W], b: &[W]) -> bool {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut ca = a.chunks_exact(8);
    let mut cb = b.chunks_exact(8);
    for (x, y) in (&mut ca).zip(&mut cb) {
        let any = O::word(x[0], y[0])
            | O::word(x[1], y[1])
            | O::word(x[2], y[2])
            | O::word(x[3], y[3])
            | O::word(x[4], y[4])
            | O::word(x[5], y[5])
            | O::word(x[6], y[6])
            | O::word(x[7], y[7]);
        if any != W::ZERO {
            return true;
        }
    }
    ca.remainder()
        .iter()
        .zip(cb.remainder())
        .any(|(&x, &y)| O::word(x, y) != W::ZERO)
}

/// Set bits of `a op b` over the words both operands have.
#[inline]
fn count_common<W: Word, O: Op>(a: &[W], b: &[W]) -> usize {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    #[cfg(target_arch = "aarch64")]
    if n >= 8 {
        // SAFETY: plain integers viewed as bytes (see `count_set_bits`).
        let (x, y) = unsafe {
            let len = core::mem::size_of_val(a);
            (
                core::slice::from_raw_parts(a.as_ptr().cast::<u8>(), len),
                core::slice::from_raw_parts(b.as_ptr().cast::<u8>(), len),
            )
        };
        return crate::arch::popcount_bytes2(x, y, O::neon);
    }
    a.iter()
        .zip(b)
        .map(|(&x, &y)| O::word(x, y).count_ones() as usize)
        .sum()
}

impl<W: Word> BitSlice<W> {
    /// Is every set bit of `self` also set in `other`?
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let a = BitSlice::new(&[0b0110u8]);
    /// let b = BitSlice::new(&[0b1110u8, 0]); // a trailing zero word changes nothing
    /// assert!(a.is_subset(b));
    /// assert!(!b.is_subset(a));
    /// ```
    #[inline]
    pub fn is_subset(&self, other: &BitSlice<W>) -> bool {
        let n = other.words.len().min(self.words.len());
        !any_common::<W, AndNot>(&self.words, &other.words) && BitSlice::new(&self.words[n..]).is_zero()
    }

    /// Is every set bit of `other` also set in `self`?
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// assert!(BitSlice::new(&[0b1110u8]).is_superset(BitSlice::new(&[0b0110u8])));
    /// ```
    #[inline]
    pub fn is_superset(&self, other: &BitSlice<W>) -> bool {
        other.is_subset(self)
    }

    /// Do `self` and `other` have no set bit in common?
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let a = BitSlice::new(&[0b0011u8]);
    /// assert!(a.is_disjoint(BitSlice::new(&[0b1100u8, 0xff])));
    /// assert!(!a.is_disjoint(BitSlice::new(&[0b0010u8])));
    /// ```
    #[inline]
    pub fn is_disjoint(&self, other: &BitSlice<W>) -> bool {
        !any_common::<W, And>(&self.words, &other.words)
    }

    /// Number of set bits in `self & other`, without building it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8]), BitSlice::new(&[0b1110u8]));
    /// assert_eq!(a.and_count(b), 2);
    /// ```
    #[inline]
    pub fn and_count(&self, other: &BitSlice<W>) -> usize {
        self.op_count::<And>(other)
    }

    /// Number of set bits in `self | other`, without building it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8]), BitSlice::new(&[0b1110u8, 1]));
    /// assert_eq!(a.or_count(b), 5);
    /// ```
    #[inline]
    pub fn or_count(&self, other: &BitSlice<W>) -> usize {
        self.op_count::<Or>(other)
    }

    /// Number of set bits in `self ^ other`, without building it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8]), BitSlice::new(&[0b1110u8]));
    /// assert_eq!(a.xor_count(b), 2);
    /// ```
    #[inline]
    pub fn xor_count(&self, other: &BitSlice<W>) -> usize {
        self.op_count::<Xor>(other)
    }

    /// Number of set bits in `self & !other`, without building it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8, 1]), BitSlice::new(&[0b1110u8]));
    /// assert_eq!(a.andnot_count(b), 2);
    /// ```
    #[inline]
    pub fn andnot_count(&self, other: &BitSlice<W>) -> usize {
        self.op_count::<AndNot>(other)
    }

    #[inline]
    fn op_count<O: Op>(&self, other: &BitSlice<W>) -> usize {
        let (a, b) = (&self.words, &other.words);
        let n = a.len().min(b.len());
        let mut total = count_common::<W, O>(a, b);
        if O::KEEP_A {
            total += BitSlice::new(&a[n..]).count_set_bits();
        }
        if O::KEEP_B {
            total += BitSlice::new(&b[n..]).count_set_bits();
        }
        total
    }

    /// The set bits of `self & other` in increasing order, without building
    /// it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8]), BitSlice::new(&[0b1110u8, 1]));
    /// assert_eq!(a.iter_and(b).collect::<Vec<_>>(), [1, 2]);
    /// // `sum`, `for_each` and `collect` use a faster internal loop than `next`.
    /// assert_eq!(a.iter_and(b).sum::<usize>(), 3);
    /// ```
    #[inline]
    pub fn iter_and<'a>(
        &'a self,
        other: &'a BitSlice<W>,
    ) -> impl FusedIterator<Item = usize> + Clone + 'a {
        OnesOf::<W, And>::new(&self.words, &other.words)
    }

    /// The set bits of `self | other` in increasing order, without building
    /// it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0101u8]), BitSlice::new(&[0b0010u8, 1]));
    /// assert_eq!(a.iter_or(b).collect::<Vec<_>>(), [0, 1, 2, 8]);
    /// ```
    #[inline]
    pub fn iter_or<'a>(
        &'a self,
        other: &'a BitSlice<W>,
    ) -> impl FusedIterator<Item = usize> + Clone + 'a {
        OnesOf::<W, Or>::new(&self.words, &other.words)
    }

    /// The set bits of `self ^ other` in increasing order, without building
    /// it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8]), BitSlice::new(&[0b1110u8, 1]));
    /// assert_eq!(a.iter_xor(b).collect::<Vec<_>>(), [0, 3, 8]);
    /// ```
    #[inline]
    pub fn iter_xor<'a>(
        &'a self,
        other: &'a BitSlice<W>,
    ) -> impl FusedIterator<Item = usize> + Clone + 'a {
        OnesOf::<W, Xor>::new(&self.words, &other.words)
    }

    /// The set bits of `self & !other` in increasing order, without building
    /// it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let (a, b) = (BitSlice::new(&[0b0111u8, 1]), BitSlice::new(&[0b1110u8]));
    /// assert_eq!(a.iter_andnot(b).collect::<Vec<_>>(), [0, 8]);
    /// ```
    #[inline]
    pub fn iter_andnot<'a>(
        &'a self,
        other: &'a BitSlice<W>,
    ) -> impl FusedIterator<Item = usize> + Clone + 'a {
        OnesOf::<W, AndNot>::new(&self.words, &other.words)
    }
}

/// Iterator over the set bits of `a op b` in increasing order. Each word of
/// the result is computed when it is reached; nothing is allocated.
#[derive(Clone)]
pub(crate) struct OnesOf<'a, W: Word, O: Op> {
    a: &'a [W],
    b: &'a [W],
    /// Words in the result: past this every word of `a op b` is zero.
    end: usize,
    idx: usize,
    cur: W,
    _op: PhantomData<O>,
}

impl<'a, W: Word, O: Op> OnesOf<'a, W, O> {
    #[inline]
    fn new(a: &'a [W], b: &'a [W]) -> Self {
        let n = a.len().min(b.len());
        let end = match (O::KEEP_A, O::KEEP_B) {
            (true, true) => a.len().max(b.len()),
            (true, false) => a.len(),
            _ => n,
        };
        let mut it = OnesOf {
            a,
            b,
            end,
            idx: 0,
            cur: W::ZERO,
            _op: PhantomData,
        };
        it.cur = it.word(0);
        it
    }

    /// Word `i` of `a op b`, `i < end` or zero.
    #[inline(always)]
    fn word(&self, i: usize) -> W {
        if i >= self.end {
            return W::ZERO;
        }
        let x = self.a.get(i).copied().unwrap_or(W::ZERO);
        let y = self.b.get(i).copied().unwrap_or(W::ZERO);
        O::word(x, y)
    }
}

impl<W: Word, O: Op> Iterator for OnesOf<'_, W, O> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.cur == W::ZERO {
            self.idx += 1;
            if self.idx >= self.end {
                self.idx = self.end;
                return None;
            }
            self.cur = self.word(self.idx);
        }
        let bit = self.cur.trailing_zeros() as usize;
        self.cur = self.cur.clear_lowest_set();
        Some(self.idx * W::BITS + bit)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.end.saturating_sub(self.idx + 1);
        (0, Some(self.cur.count_ones() as usize + left * W::BITS))
    }

    /// One pass over the words both operands have, then over the words of
    /// the longer one if they pass through: no per-word bounds checks or
    /// `Option` state.
    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, usize) -> B,
    {
        let mut acc = decode_word(init, &mut f, self.cur, self.idx * W::BITS);
        let start = self.idx + 1;
        if start >= self.end {
            return acc;
        }
        let n = self.a.len().min(self.b.len());
        if start < n {
            let (a, b) = (&self.a[start..n], &self.b[start..n]);
            for (j, (&x, &y)) in a.iter().zip(b).enumerate() {
                acc = decode_word(acc, &mut f, O::word(x, y), (start + j) * W::BITS);
            }
        }
        // the words only one operand has: kept as they are, or the loop is
        // empty (`end == n`)
        let from = start.max(n);
        let tail = if self.a.len() > n { self.a } else { self.b };
        for (j, &w) in tail[from.min(self.end)..self.end].iter().enumerate() {
            acc = decode_word(acc, &mut f, w, (from + j) * W::BITS);
        }
        acc
    }
}

impl<W: Word, O: Op> FusedIterator for OnesOf<'_, W, O> {}
