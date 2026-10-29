// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shifts and rotations of a whole [`BitSlice`], within its length. Whole
//! words move with `copy_within` (a memmove); a remaining shift of less than
//! a word is one pass that combines each word with its neighbour.

use crate::{BitSlice, Word};

impl<W: Word> BitSlice<W> {
    /// Move every bit `n` positions up, toward higher indices, like `<<` on
    /// an integer: bit `i` goes to `i + n`. Bits pushed past
    /// [`len_bits`](BitSlice::len_bits) are lost and the low `n` bits become
    /// zero. This is the opposite direction from `bitvec`'s `shift_left`,
    /// which moves toward index 0.
    ///
    /// On a [`Bitmap`](crate::Bitmap) the growing
    /// [`Bitmap::shift_left`](crate::Bitmap::shift_left) takes precedence.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b1000_0001u8, 0];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.shift_left(3);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [3, 10]);
    /// view.shift_left(10); // bit 10 + 10 is past the end
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [13]);
    /// ```
    pub fn shift_left(&mut self, n: usize) {
        let w = &mut self.words;
        let len = w.len();
        let (q, r) = (n / W::BITS, n % W::BITS);
        if q >= len {
            w.fill(W::ZERO);
            return;
        }
        if r == 0 {
            w.copy_within(0..len - q, q);
        } else {
            // From the top down, so each source word is read before it is
            // overwritten.
            for i in (q + 1..len).rev() {
                w[i] = (w[i - q] << r) | (w[i - q - 1] >> (W::BITS - r));
            }
            w[q] = w[0] << r;
        }
        w[..q].fill(W::ZERO);
    }

    /// Move every bit `n` positions down, toward lower indices, like `>>` on
    /// an integer: bit `i` goes to `i - n`, the low `n` bits are dropped and
    /// the top `n` bits within [`len_bits`](BitSlice::len_bits) become zero.
    ///
    /// On a [`Bitmap`](crate::Bitmap) the trimming
    /// [`Bitmap::shift_right`](crate::Bitmap::shift_right) takes precedence.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b1000_0001u8, 0b1];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.shift_right(7);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [0, 1]);
    /// ```
    pub fn shift_right(&mut self, n: usize) {
        let w = &mut self.words;
        let len = w.len();
        let (q, r) = (n / W::BITS, n % W::BITS);
        if q >= len {
            w.fill(W::ZERO);
            return;
        }
        if r == 0 {
            w.copy_within(q..len, 0);
        } else {
            // From the bottom up, so each source word is read before it is
            // overwritten.
            for i in 0..len - q - 1 {
                w[i] = (w[i + q] >> r) | (w[i + q + 1] << (W::BITS - r));
            }
            w[len - q - 1] = w[len - 1] >> r;
        }
        w[len - q..].fill(W::ZERO);
    }

    /// Rotate the bits `n` positions up within
    /// [`len_bits`](BitSlice::len_bits): bit `i` goes to
    /// `(i + n) % len_bits()`, like `rotate_left` on an integer.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b1000_0001u8, 0];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.rotate_left(9);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [0, 9]);
    /// ```
    pub fn rotate_left(&mut self, n: usize) {
        let len = self.len_bits();
        if len == 0 {
            return;
        }
        let n = n % len;
        let w = &mut self.words;
        let (q, r) = (n / W::BITS, n % W::BITS);
        w.rotate_right(q);
        if r != 0 {
            let carry = w[w.len() - 1] >> (W::BITS - r);
            for i in (1..w.len()).rev() {
                w[i] = (w[i] << r) | (w[i - 1] >> (W::BITS - r));
            }
            w[0] = (w[0] << r) | carry;
        }
    }

    /// Rotate the bits `n` positions down within
    /// [`len_bits`](BitSlice::len_bits): bit `i` goes to
    /// `(i - n) mod len_bits()`, like `rotate_right` on an integer.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0b1000_0001u8, 0];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.rotate_right(1);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [6, 15]);
    /// ```
    pub fn rotate_right(&mut self, n: usize) {
        let len = self.len_bits();
        if len == 0 {
            return;
        }
        self.rotate_left(len - n % len);
    }
}
