// SPDX-License-Identifier: MIT OR Apache-2.0
use core::iter::FusedIterator;

use crate::Word;
#[cfg(target_arch = "x86_64")]
use crate::arch::{opaque, place_loop};

/// Iterator over set bits in increasing order, see
/// [`BitSlice::iter_ones`](crate::BitSlice::iter_ones).
///
/// # Examples
///
/// ```
/// use vbitmap::BitSlice;
///
/// let words = [0b101u8, 0b1];
/// let mut ones = BitSlice::new(&words).iter_ones();
/// assert_eq!(ones.next(), Some(0));
/// assert_eq!(ones.size_hint(), (2, Some(2))); // how many are left
/// assert_eq!(ones.collect::<Vec<_>>(), [2, 8]);
///
/// // `sum`, `for_each` and `collect` use a faster internal loop than `next`.
/// let total: usize = BitSlice::new(&words).iter_ones().sum();
/// assert_eq!(total, 0 + 2 + 8);
/// ```
#[derive(Clone)]
pub struct Ones<'a, W: Word> {
    pub(crate) words: &'a [W],
    pub(crate) idx: usize,
    pub(crate) cur: W,
}

impl<W: Word> Ones<'_, W> {
    /// Set bits not yet yielded.
    fn remaining(&self) -> usize {
        let rest = self.words.get(self.idx + 1..).unwrap_or(&[]);
        self.cur.count_ones() as usize + rest.iter().map(|w| w.count_ones() as usize).sum::<usize>()
    }
}

impl<W: Word> Iterator for Ones<'_, W> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.cur == W::ZERO {
            self.idx += 1;
            self.cur = *self.words.get(self.idx)?;
        }
        let bit = self.cur.trailing_zeros() as usize;
        self.cur = self.cur.clear_lowest_set();
        Some(self.idx * W::BITS + bit)
    }

    /// Exact, computed with one popcount pass over the remaining words.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.remaining();
        (n, Some(n))
    }

    #[inline]
    fn count(self) -> usize {
        self.remaining()
    }

    /// Internal iteration: a plain loop over the words, with no per-bit
    /// `Option` state machine. `sum`, `for_each`, `collect` and friends use it.
    #[inline]
    fn fold<B, F>(self, init: B, f: F) -> B
    where
        F: FnMut(B, usize) -> B,
    {
        // x86-64 and aarch64: a short bitmap runs one flat loop. On x86-64
        // everything else (empty run skipping) sits out of line, so the short
        // case does not carry the long path's six-register frame; on aarch64
        // the extra call made the short case slower, so it stays inline.
        // Other targets inline the general loop.
        #[cfg(target_arch = "x86_64")]
        {
            if self.words.len() - self.idx.min(self.words.len()) >= 8 {
                // fields as arguments, so they travel in registers: passing
                // `self` by reference would store it to the stack first
                Ones::fold_long(self.words, self.idx, self.cur, init, f)
            } else {
                self.fold_short(init, f)
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if self.words.len() - self.idx.min(self.words.len()) >= 8 {
                self.fold_impl(init, f)
            } else {
                self.fold_short(init, f)
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            self.fold_impl(init, f)
        }
    }
}

impl<W: Word> Ones<'_, W> {
    #[cfg(target_arch = "x86_64")]
    #[inline(never)]
    fn fold_long<B, F: FnMut(B, usize) -> B>(words: &[W], idx: usize, cur: W, init: B, f: F) -> B {
        Ones { words, idx, cur }.fold_impl(init, f)
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    fn fold_short<B, F: FnMut(B, usize) -> B>(self, init: B, mut f: F) -> B {
        let mut acc = init;
        let mut cur = self.cur;
        let mut idx = self.idx;
        let mut base = idx * W::BITS;
        place_loop::<16>();
        loop {
            // The first bit of a word on its own, so a word with one bit
            // (most words of a sparse bitmap) leaves with a single branch;
            // the rest two bits per pass. A loop of one bit per pass is about
            // 18 bytes and runs at half speed wherever it straddles a 64-byte
            // fetch block; this one runs the same anywhere.
            //
            // Opaque, so LLVM cannot reassociate the word base into the
            // accumulator (`(acc + base) + bit`): that puts two adds and a
            // move on the per-bit dependency chain of `sum`.
            if cur != W::ZERO {
                acc = f(acc, opaque(base + cur.trailing_zeros() as usize));
                cur = cur.clear_lowest_set();
                while cur != W::ZERO {
                    acc = f(acc, opaque(base + cur.trailing_zeros() as usize));
                    cur = cur.clear_lowest_set();
                    if cur == W::ZERO {
                        break;
                    }
                    acc = f(acc, opaque(base + cur.trailing_zeros() as usize));
                    cur = cur.clear_lowest_set();
                }
            }
            idx += 1;
            match self.words.get(idx) {
                Some(&w) => cur = w,
                None => return acc,
            }
            base += W::BITS;
        }
    }

    /// The word we are in, then the rest, one bit per pass, with the word
    /// base carried along instead of recomputed: fewer instructions per
    /// word than the general loop, which matters for a sparse bitmap.
    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn fold_short<B, F: FnMut(B, usize) -> B>(self, init: B, mut f: F) -> B {
        let mut acc = init;
        let mut base = self.idx * W::BITS;
        let mut cur = self.cur;
        while cur != W::ZERO {
            acc = f(acc, base + cur.trailing_zeros() as usize);
            cur = cur.clear_lowest_set();
        }
        for &w in self.words.get(self.idx + 1..).unwrap_or(&[]) {
            base += W::BITS;
            let mut cur = w;
            while cur != W::ZERO {
                acc = f(acc, base + cur.trailing_zeros() as usize);
                cur = cur.clear_lowest_set();
            }
        }
        acc
    }

    #[inline(always)]
    fn fold_impl<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, usize) -> B,
    {
        let mut acc = init;
        let mut cur = self.cur;
        let mut idx = self.idx;
        // finish the word we are in
        while cur != W::ZERO {
            let bit = cur.trailing_zeros() as usize;
            cur = cur.clear_lowest_set();
            acc = f(acc, idx * W::BITS + bit);
        }
        idx += 1;
        let rest = self.words.get(idx..).unwrap_or(&[]);

        #[cfg(target_arch = "aarch64")]
        if W::BITS == 64 && rest.len() >= DENSE_SAMPLE_WORDS && looks_dense(rest) {
            return fold_dense(rest, idx, acc, f);
        }

        if rest.len() < 8 {
            for (j, &w) in rest.iter().enumerate() {
                acc = decode_word(acc, &mut f, w, (idx + j) * W::BITS);
            }
            return acc;
        }

        // Skip runs of empty words eight at a time: a sparse bitmap is mostly
        // zero words, and one OR over a cache line settles eight of them.
        let mut chunks = rest.chunks_exact(8);
        #[cfg(target_arch = "x86_64")]
        place_loop::<32>();
        for chunk in &mut chunks {
            let any = chunk[0]
                | chunk[1]
                | chunk[2]
                | chunk[3]
                | chunk[4]
                | chunk[5]
                | chunk[6]
                | chunk[7];
            if any != W::ZERO {
                for (j, &w) in chunk.iter().enumerate() {
                    acc = decode_word(acc, &mut f, w, (idx + j) * W::BITS);
                }
            }
            idx += 8;
        }
        for (j, &w) in chunks.remainder().iter().enumerate() {
            acc = decode_word(acc, &mut f, w, (idx + j) * W::BITS);
        }
        acc
    }
}

/// Feed the set bits of one word, in increasing order, to `f`.
#[inline(always)]
pub(crate) fn decode_word<W: Word, B, F: FnMut(B, usize) -> B>(
    mut acc: B,
    f: &mut F,
    w: W,
    base: usize,
) -> B {
    let mut cur = w;
    while cur != W::ZERO {
        let bit = cur.trailing_zeros() as usize;
        cur = cur.clear_lowest_set();
        acc = f(acc, base + bit);
    }
    acc
}

/// Words sampled to decide whether the dense decoder pays off.
#[cfg(target_arch = "aarch64")]
const DENSE_SAMPLE_WORDS: usize = 32;

/// More than three set bits per byte in the first words.
#[cfg(target_arch = "aarch64")]
fn looks_dense<W: Word>(words: &[W]) -> bool {
    let sample = &words[..DENSE_SAMPLE_WORDS];
    let bytes = DENSE_SAMPLE_WORDS * (W::BITS / 8);
    // SAFETY: plain integers viewed as bytes (see `count_set_bits`).
    let raw = unsafe { core::slice::from_raw_parts(sample.as_ptr().cast::<u8>(), bytes) };
    crate::arch::popcount_bytes(raw) > 3 * bytes
}

/// Decode 64-bit words that are mostly ones. Clearing the lowest set bit is
/// a two-instruction dependency per bit (`sub`, `and`) on aarch64, which makes
/// one long chain the bottleneck. Each word is cut into a low and a high half
/// whose chains run side by side: the high half's positions are buffered while
/// the low half is emitted, so bits still come out in increasing order.
#[cfg(target_arch = "aarch64")]
fn fold_dense<W: Word, B, F: FnMut(B, usize) -> B>(
    words: &[W],
    first: usize,
    init: B,
    mut f: F,
) -> B {
    let mut acc = init;
    for (k, &w) in words.iter().enumerate() {
        let base = (first + k) * 64;
        let w = w.to_u64();
        let mut lo = w & 0xFFFF_FFFF;
        let mut hi = w >> 32;
        let mut buf = [0u8; 32];
        let mut n = 0;
        while lo != 0 {
            acc = f(acc, base + lo.trailing_zeros() as usize);
            lo &= lo - 1;
            if hi != 0 {
                buf[n] = hi.trailing_zeros() as u8;
                n += 1;
                hi &= hi - 1;
            }
        }
        for &b in &buf[..n] {
            acc = f(acc, base + 32 + b as usize);
        }
        while hi != 0 {
            acc = f(acc, base + 32 + hi.trailing_zeros() as usize);
            hi &= hi - 1;
        }
    }
    acc
}

impl<W: Word> FusedIterator for Ones<'_, W> {}

/// Iterator over the clear bits within
/// [`len_bits`](crate::BitSlice::len_bits), in increasing order, see
/// [`BitSlice::iter_zeros`](crate::BitSlice::iter_zeros).
///
/// # Examples
///
/// ```
/// use vbitmap::BitSlice;
///
/// let words = [0b1111_1010u8];
/// let mut zeros = BitSlice::new(&words).iter_zeros();
/// assert_eq!(zeros.next(), Some(0));
/// assert_eq!(zeros.collect::<Vec<_>>(), [2]);
/// ```
#[derive(Clone)]
pub struct Zeros<'a, W: Word> {
    pub(crate) words: &'a [W],
    pub(crate) idx: usize,
    /// The clear bits of word `idx` not yet yielded, as set bits.
    pub(crate) cur: W,
}

impl<W: Word> Iterator for Zeros<'_, W> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.cur == W::ZERO {
            self.idx += 1;
            self.cur = !*self.words.get(self.idx)?;
        }
        let bit = self.cur.trailing_zeros() as usize;
        self.cur = self.cur.clear_lowest_set();
        Some(self.idx * W::BITS + bit)
    }

    /// Exact, computed with one popcount pass over the remaining words.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let rest = self.words.get(self.idx + 1..).unwrap_or(&[]);
        let n = self.cur.count_ones() as usize + crate::BitSlice::new(rest).count_clear_bits();
        (n, Some(n))
    }

    /// A plain loop over the complemented words.
    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, usize) -> B,
    {
        let mut acc = decode_word(init, &mut f, self.cur, self.idx * W::BITS);
        let start = self.idx + 1;
        for (j, &w) in self.words.get(start..).unwrap_or(&[]).iter().enumerate() {
            acc = decode_word(acc, &mut f, !w, (start + j) * W::BITS);
        }
        acc
    }
}

impl<W: Word> FusedIterator for Zeros<'_, W> {}

impl<W: Word> crate::BitSlice<W> {
    /// Iterate over the clear bits within [`len_bits`](crate::BitSlice::len_bits),
    /// in increasing order. The bits past the end are clear too, but endless,
    /// so they are not yielded.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [!0b0100_0001u8, 0xff];
    /// let zeros: Vec<usize> = BitSlice::new(&words).iter_zeros().collect();
    /// assert_eq!(zeros, [0, 6]);
    /// ```
    #[inline]
    pub fn iter_zeros(&self) -> Zeros<'_, W> {
        Zeros {
            words: &self.words,
            idx: 0,
            cur: self.words.first().map_or(W::ZERO, |&w| !w),
        }
    }
}
