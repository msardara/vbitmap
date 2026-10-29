// SPDX-License-Identifier: MIT OR Apache-2.0
//! Single-bit and whole-bitmap updates and counts that need no allocation:
//! toggle, complement, fill, reverse, counts over a range, rank and select.

use crate::arch::in_register;
use crate::{BitSlice, Word};

impl<W: Word> BitSlice<W> {
    /// Flip bit `i`. Panics if out of range, never grows or trims.
    ///
    /// On a [`Bitmap`](crate::Bitmap) the growing
    /// [`Bitmap::toggle`](crate::Bitmap::toggle) takes precedence.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u64; 2];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.toggle(70);
    /// assert!(view.get(70));
    /// view.toggle(70);
    /// assert!(view.is_zero());
    /// ```
    #[inline]
    pub fn toggle(&mut self, i: usize) {
        let w = &mut self.words[i / W::BITS];
        // `in_register`: a register `btc` and a plain store on x86-64, see
        // `set`
        *w = in_register(*w) ^ W::bit(i % W::BITS);
    }

    /// Flip every bit, within [`len_bits`](BitSlice::len_bits).
    ///
    /// A bitmap is zero-extended without end, so its complement would be
    /// infinite: this complements the stored words only, and the bits past
    /// the end still read as zero. Never grows or trims, so inverting twice
    /// gives back the same words. To complement within a chosen number of
    /// bits, size the bitmap first with
    /// [`Bitmap::resize_bits`](crate::Bitmap::resize_bits).
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b0000_1111u8, 0];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.invert();
    /// assert_eq!(view.as_words(), &[0b1111_0000, 0xff]);
    /// assert!(!view.get(16)); // past the end: still zero
    /// ```
    #[inline]
    pub fn invert(&mut self) {
        for w in &mut self.words {
            *w = !*w;
        }
    }

    /// Set every bit, within [`len_bits`](BitSlice::len_bits). The
    /// counterpart of [`zero`](BitSlice::zero).
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u16; 3];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.set_all();
    /// assert_eq!(view.count_set_bits(), 48);
    /// ```
    #[inline]
    pub fn set_all(&mut self) {
        self.words.fill(W::ONES);
    }

    /// Reverse the order of all stored bits in place.
    ///
    /// Bit zero becomes the highest stored bit, and the highest stored bit
    /// becomes bit zero. The operation preserves the number of words and
    /// allocates nothing. On a [`Bitmap`](crate::Bitmap), this reverses its
    /// currently stored words, including zero bits in the final word. It does
    /// not use or change the bitmap's allocation capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b1001u8, 0b0000_0010];
    /// BitSlice::new_mut(&mut words).reverse();
    /// assert_eq!(words, [0b0100_0000, 0b1001_0000]);
    /// ```
    #[inline]
    pub fn reverse(&mut self) {
        let mut lo = 0;
        let mut hi = self.words.len();
        while lo < hi {
            hi -= 1;
            if lo == hi {
                self.words[lo] = self.words[lo].reverse_bits();
                break;
            }
            let a = self.words[lo].reverse_bits();
            let b = self.words[hi].reverse_bits();
            self.words[lo] = b;
            self.words[hi] = a;
            lo += 1;
        }
    }

    /// Number of clear bits within [`len_bits`](BitSlice::len_bits).
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// assert_eq!(BitSlice::new(&[0b1011u8, 0]).count_clear_bits(), 13);
    /// ```
    #[inline]
    pub fn count_clear_bits(&self) -> usize {
        self.len_bits() - self.count_set_bits()
    }

    /// Number of set bits among bits `i..i + n_bits`. Bits past the end read
    /// as zero, so the range may run past it.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0xffu8, 0x0f];
    /// let view = BitSlice::new(&words);
    /// assert_eq!(view.count_region(4, 8), 8); // bits 4 to 11
    /// assert_eq!(view.count_region(10, 1_000), 2); // runs past the end
    /// ```
    #[inline]
    pub fn count_region(&self, i: usize, n_bits: usize) -> usize {
        let len = self.len_bits();
        if i >= len {
            return 0;
        }
        let end = i.saturating_add(n_bits).min(len);
        if end == i {
            return 0;
        }
        let (a0, a1) = (i / W::BITS, i % W::BITS);
        let (b0, b1) = (end / W::BITS, end % W::BITS);
        let w = &self.words;
        if a0 == b0 {
            // one word, `b1 > a1`
            return ((w[a0] >> a1) & W::low_mask(b1 - a1)).count_ones() as usize;
        }
        let mut n = (w[a0] >> a1).count_ones() as usize;
        // the full words in between, with the fast popcount
        n += BitSlice::new(&w[a0 + 1..b0]).count_set_bits();
        if b1 > 0 {
            n += (w[b0] & W::low_mask(b1)).count_ones() as usize;
        }
        n
    }

    /// Number of set bits below bit `i` (bits `0..i`), any `i`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let view = BitSlice::new(&[0b1011_0110u8, 1]);
    /// assert_eq!(view.rank(0), 0);
    /// assert_eq!(view.rank(3), 2); // bits 1 and 2
    /// assert_eq!(view.rank(1_000), 6);
    /// ```
    #[inline]
    pub fn rank(&self, i: usize) -> usize {
        self.count_region(0, i)
    }

    /// Position of the set bit of rank `n` (the `n + 1`-th set bit, counting
    /// from zero), or `None` if fewer than `n + 1` bits are set. The inverse
    /// of [`rank`](BitSlice::rank): `rank(select(n)) == n`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let view = BitSlice::new(&[0b1011_0110u8, 1]);
    /// assert_eq!(view.select(0), Some(1));
    /// assert_eq!(view.select(5), Some(8));
    /// assert_eq!(view.select(6), None);
    /// ```
    #[inline]
    pub fn select(&self, n: usize) -> Option<usize> {
        let mut n = n;
        let w = &self.words;
        // Skip eight words per step on their popcount, then one word at a time.
        let mut i = 0;
        while i + 8 <= w.len() {
            let c: usize = w[i..i + 8].iter().map(|x| x.count_ones() as usize).sum();
            if n < c {
                break;
            }
            n -= c;
            i += 8;
        }
        while i < w.len() {
            let c = w[i].count_ones() as usize;
            if n < c {
                return Some(i * W::BITS + select_in_word(w[i], n));
            }
            n -= c;
            i += 1;
        }
        None
    }
}

/// Position of the set bit of rank `n` in `w`, `n < w.count_ones()`.
#[inline]
fn select_in_word<W: Word>(w: W, n: usize) -> usize {
    if W::BITS > 64 {
        let lo = w.to_u64();
        let c = lo.count_ones() as usize;
        if n < c {
            return select_u64(lo, n);
        }
        return 64 + select_u64((w >> 64).to_u64(), n - c);
    }
    select_u64(w.to_u64(), n)
}

/// Position of the set bit of rank `n` in `x`, `n < x.count_ones()`.
#[inline]
fn select_u64(x: u64, n: usize) -> usize {
    crate::arch::select_u64(x, n)
}
