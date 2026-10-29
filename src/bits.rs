// SPDX-License-Identifier: MIT OR Apache-2.0
//! Single-bit and whole-bitmap updates and counts that need no allocation:
//! toggle, complement, fill, counts over a range, rank and select.

use crate::slice::in_register;
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
    // BMI2: deposit a single bit at the `n`-th set position.
    #[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
    {
        // SAFETY: the intrinsic needs BMI2, which this build enables.
        unsafe { core::arch::x86_64::_pdep_u64(1 << n, x).trailing_zeros() as usize }
    }
    // Elsewhere, broadword: running byte popcounts locate the byte without a
    // loop, and a table finishes inside it.
    #[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
    {
        const L8: u64 = 0x0101_0101_0101_0101;
        const H8: u64 = 0x8080_8080_8080_8080;
        let mut s = x - ((x >> 1) & 0x5555_5555_5555_5555);
        s = (s & 0x3333_3333_3333_3333) + ((s >> 2) & 0x3333_3333_3333_3333);
        s = (s + (s >> 4)) & 0x0f0f_0f0f_0f0f_0f0f;
        // byte k holds the number of set bits in bytes 0..=k (at most 64)
        let cum = s.wrapping_mul(L8);
        // high bit of byte k set when `n >= cum[k]`: those bytes come before
        // the one we want (no borrow crosses a byte: `cum[k] <= 64 < 128`)
        let before = (((n as u64 * L8) | H8) - cum) & H8;
        let byte = ((before >> 7).wrapping_mul(L8) >> 56) as usize;
        let below = ((cum << 8) >> (8 * byte)) as usize & 0xff;
        let b = (x >> (8 * byte)) as usize & 0xff;
        8 * byte + SELECT_IN_BYTE[b * 8 + (n - below)] as usize
    }
}

/// `SELECT_IN_BYTE[b * 8 + r]`: position of the set bit of rank `r` in the
/// byte `b`.
#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
static SELECT_IN_BYTE: [u8; 2048] = {
    let mut t = [0u8; 2048];
    let mut b = 0;
    while b < 256 {
        let mut r = 0;
        let mut pos = 0;
        while pos < 8 {
            if b & (1 << pos) != 0 {
                t[b * 8 + r] = pos as u8;
                r += 1;
            }
            pos += 1;
        }
        b += 1;
    }
    t
};
