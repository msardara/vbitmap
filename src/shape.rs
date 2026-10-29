// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`Bitmap`] updates that grow, shrink or move the words: toggles, shifts,
//! resizing, splitting and joining, and conversion from and to bytes.

use alloc::vec::Vec;

use crate::slice::in_register;
use crate::storage::AlignedWords;
use crate::{BitSlice, Bitmap, Word};

impl<W: Word> Bitmap<W> {
    /// Flip bit `i`, growing as needed. Never trims, like
    /// [`Bitmap::unset`].
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.toggle(100); // grows
    /// assert!(b.get(100));
    /// b.toggle(100);
    /// assert!(b.is_zero());
    /// ```
    #[inline]
    pub fn toggle(&mut self, i: usize) {
        let i0 = i / W::BITS;
        self.validate_word(i0);
        // SAFETY: validate_word made `i0` a valid index.
        let w = unsafe { self.words.get_unchecked_mut(i0) };
        *w = in_register(*w) ^ W::bit(i % W::BITS);
    }

    /// Flip `n_bits` bits starting at `i`, growing as needed, no trimming.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.set_region(0, true, 10);
    /// b.toggle_region(5, 10); // bits 5 to 14
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 1, 2, 3, 4, 10, 11, 12, 13, 14]);
    /// ```
    pub fn toggle_region(&mut self, i: usize, n_bits: usize) {
        if n_bits == 0 {
            return;
        }
        let end = i.checked_add(n_bits - 1).expect("bit index overflow");
        let (a0, a1) = (i / W::BITS, i % W::BITS);
        let (b0, b1) = (end / W::BITS, end % W::BITS);
        self.validate_word(b0);
        let w = &mut self.words[..];
        if a0 == b0 {
            w[a0] = w[a0] ^ (W::low_mask(n_bits) << a1);
            return;
        }
        w[a0] = w[a0] ^ (W::ONES << a1);
        // a plain loop over whole words, which vectorizes
        for x in &mut w[a0 + 1..b0] {
            *x = !*x;
        }
        w[b0] = w[b0] ^ (W::ONES >> (W::BITS - b1 - 1));
    }

    /// Move every bit `n` positions up, like `<<` on an integer, growing so
    /// that no bit is lost. See [`BitSlice::shift_left`] for the version
    /// within the current length.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b: NativeBitmap = [0usize, 3].into_iter().collect();
    /// b.shift_left(100);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [100, 103]);
    /// ```
    pub fn shift_left(&mut self, n: usize) {
        let Some(last) = self.last_set() else {
            return;
        };
        let top = last.checked_add(n).expect("bit index overflow");
        self.validate_word(top / W::BITS);
        // only the words up to the top one hold bits after the shift
        let used = top / W::BITS + 1;
        BitSlice::new_mut(&mut self.words[..used]).shift_left(n);
    }

    /// Move every bit `n` positions down, like `>>` on an integer: the low
    /// `n` bits are dropped. Trims. See [`BitSlice::shift_right`].
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b: NativeBitmap = [3usize, 100].into_iter().collect();
    /// b.shift_right(10);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [90]);
    /// assert_eq!(b.len_words(), 2); // trimmed
    /// ```
    pub fn shift_right(&mut self, n: usize) {
        BitSlice::new_mut(&mut self.words).shift_right(n);
        self.trim();
    }

    /// Clear the bits at and above `n_bits`, then trim. Never grows.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b: NativeBitmap = [1usize, 70, 200].into_iter().collect();
    /// b.truncate_bits(71);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [1, 70]);
    /// ```
    pub fn truncate_bits(&mut self, n_bits: usize) {
        self.cut(n_bits);
        self.trim();
    }

    /// Make the bitmap exactly `n_bits.div_ceil(W::BITS)` words long,
    /// zero-filled when it grows, and clear the bits at and above `n_bits`.
    /// Never trims, so this is the way to fix the length that
    /// [`invert`](BitSlice::invert), [`iter_zeros`](BitSlice::iter_zeros) and
    /// [`count_clear_bits`](BitSlice::count_clear_bits) work within.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::Bitmap64;
    ///
    /// let mut b = Bitmap64::new();
    /// b.resize_bits(100);
    /// b.invert(); // complement within 128 bits...
    /// b.truncate_bits(100); // ...then keep 100 of them
    /// assert_eq!(b.count_set_bits(), 100);
    /// ```
    pub fn resize_bits(&mut self, n_bits: usize) {
        let n = n_bits.div_ceil(W::BITS);
        if n > self.words.len() {
            self.words.resize_zeroed(n);
        }
        self.cut(n_bits);
    }

    /// Drop the words past bit `n_bits` and clear the bits at and above it
    /// in the last one.
    fn cut(&mut self, n_bits: usize) {
        self.words.truncate(n_bits.div_ceil(W::BITS));
        let r = n_bits % W::BITS;
        if r != 0 && self.words.len() == n_bits.div_ceil(W::BITS) {
            let last = self.words.len() - 1;
            self.words[last] = self.words[last] & W::low_mask(r);
        }
    }

    /// Split at bit `at`: `self` keeps the bits below `at` (trimmed) and the
    /// bits at and above it are returned, moved down so that bit `at` is bit
    /// 0 of the result.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b: NativeBitmap = [1usize, 70, 200].into_iter().collect();
    /// let tail = b.split_off(70);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [1]);
    /// assert_eq!(tail.iter_ones().collect::<Vec<_>>(), [0, 130]);
    /// ```
    pub fn split_off(&mut self, at: usize) -> Self {
        let q = at / W::BITS;
        let mut tail = Self::new();
        if q < self.words.len() {
            tail.words = AlignedWords::from_slice(&self.words[q..]);
            tail.shift_right(at % W::BITS);
        }
        self.truncate_bits(at);
        tail
    }

    /// Append the words of `other` after the last word: bit `i` of `other`
    /// becomes bit `self.len_bits() + i`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::{BitSlice, Bitmap};
    ///
    /// let mut b = Bitmap::<u8>::from_words(vec![0b1]);
    /// b.append(BitSlice::new(&[0b10u8]));
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 9]);
    /// ```
    pub fn append(&mut self, other: &BitSlice<W>) {
        self.words.extend_from_slice(&other.words);
    }

    /// Reserve room for at least `additional` more bits past
    /// [`len_bits`](BitSlice::len_bits) without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::new();
    /// b.reserve_bits(1000);
    /// assert!(b.capacity_bits() >= 1000);
    /// assert!(!b.will_expand(999));
    /// ```
    pub fn reserve_bits(&mut self, additional: usize) {
        let words = self.words.len() + additional.div_ceil(W::BITS);
        self.words.reserve_total(words);
    }

    /// Number of bits the bitmap can hold without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// assert!(NativeBitmap::with_capacity_bits(300).capacity_bits() >= 300);
    /// ```
    #[inline]
    pub fn capacity_bits(&self) -> usize {
        self.words.capacity() * W::BITS
    }

    /// Release the memory not needed for the current words.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b = NativeBitmap::with_capacity_bits(100_000);
    /// b.set(3);
    /// b.shrink_to_fit();
    /// assert!(b.capacity_bits() < 100_000);
    /// assert!(b.get(3));
    /// ```
    pub fn shrink_to_fit(&mut self) {
        self.words.shrink_to_fit();
    }

    /// Keep only the set bits `i` for which `keep(i)` is true, in increasing
    /// order of `i`, then trim. Zero words are skipped without a call.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::NativeBitmap;
    ///
    /// let mut b: NativeBitmap = (0..10).collect();
    /// b.retain(|i| i % 3 == 0);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 3, 6, 9]);
    /// ```
    pub fn retain(&mut self, mut keep: impl FnMut(usize) -> bool) {
        for (k, w) in self.words.iter_mut().enumerate() {
            let mut left = *w;
            let mut out = *w;
            while left != W::ZERO {
                let b = left.trailing_zeros() as usize;
                left = left.clear_lowest_set();
                if !keep(k * W::BITS + b) {
                    out = out & !W::bit(b);
                }
            }
            *w = out;
        }
        self.trim();
    }

    /// Build from bytes, bit `i` being bit `i % 8` of `bytes[i / 8]`: the
    /// little-endian layout of the words, whatever the word type or target.
    /// The bitmap is `bytes.len()` bytes long, rounded up to whole words, and
    /// is not trimmed. (`bit-vec`'s `from_bytes` puts bit 0 in the most
    /// significant bit of each byte instead.)
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::Bitmap64;
    ///
    /// let b = Bitmap64::from_bytes(&[0b101, 0, 0, 0, 0, 0, 0, 0, 1]);
    /// assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 2, 64]);
    /// assert_eq!(b.len_words(), 2);
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut b = Self::alloc(bytes.len() * 8);
        #[cfg(target_endian = "little")]
        // SAFETY: the words cover `bytes.len()` bytes (rounded up), are
        // exclusively borrowed and accept any byte pattern; on a
        // little-endian target their bytes are in bit order.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                b.words.as_mut_ptr().cast::<u8>(),
                bytes.len(),
            );
        }
        #[cfg(not(target_endian = "little"))]
        for (w, chunk) in b.words.iter_mut().zip(bytes.chunks(W::BITS / 8)) {
            for (k, &x) in chunk.iter().enumerate() {
                *w = *w | (W::from_u64(x as u64) << (8 * k));
            }
        }
        b
    }
}

impl<W: Word> BitSlice<W> {
    /// The words as bytes, bit `i` being bit `i % 8` of byte `i / 8` (the
    /// little-endian layout of the words, whatever the target). The inverse
    /// of [`Bitmap::from_bytes`].
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// assert_eq!(BitSlice::new(&[0x0102u16]).to_bytes(), [2, 1]);
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        #[cfg(target_endian = "little")]
        {
            self.as_le_bytes().to_vec()
        }
        #[cfg(not(target_endian = "little"))]
        {
            let mut out = Vec::with_capacity(self.bytes());
            for &w in &self.words {
                for k in 0..W::BITS / 8 {
                    out.push((w >> (8 * k)).to_u64() as u8);
                }
            }
            out
        }
    }
}
