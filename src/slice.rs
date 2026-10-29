// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Ones, Word};

/// First non-zero word and its index. Bitmaps shorter than eight words are
/// scanned directly; longer ones eight words per step so the OR-reduction
/// vectorizes.
#[inline]
pub(crate) fn first_nonzero<W: Word>(w: &[W]) -> Option<(usize, W)> {
    let mut i = 0;
    if w.len() >= 8 {
        while i + 8 <= w.len() {
            let c = &w[i..i + 8];
            let any = c[0] | c[1] | c[2] | c[3] | c[4] | c[5] | c[6] | c[7];
            if any != W::ZERO {
                break;
            }
            i += 8;
        }
    }
    while i < w.len() {
        let x = w[i];
        if x != W::ZERO {
            return Some((i, x));
        }
        i += 1;
    }
    None
}

/// First word that is not all ones, with its index; same scan as
/// [`first_nonzero`].
#[inline]
pub(crate) fn first_not_ones<W: Word>(w: &[W]) -> Option<(usize, W)> {
    let mut i = 0;
    if w.len() >= 8 {
        while i + 8 <= w.len() {
            let c = &w[i..i + 8];
            let all = c[0] & c[1] & c[2] & c[3] & c[4] & c[5] & c[6] & c[7];
            if all != W::ONES {
                break;
            }
            i += 8;
        }
    }
    while i < w.len() {
        let x = w[i];
        if x != W::ONES {
            return Some((i, x));
        }
        i += 1;
    }
    None
}

/// Length of `w` without trailing zero words, scanning backwards eight words
/// per step.
#[inline]
pub(crate) fn nonzero_len<W: Word>(w: &[W]) -> usize {
    if w.len() < 8 {
        return w.iter().rposition(|&x| x != W::ZERO).map_or(0, |p| p + 1);
    }
    let mut end = w.len();
    while end >= 8 {
        let c = &w[end - 8..end];
        let any = c[0] | c[1] | c[2] | c[3] | c[4] | c[5] | c[6] | c[7];
        if any != W::ZERO {
            break;
        }
        end -= 8;
    }
    while end > 0 && w[end - 1] == W::ZERO {
        end -= 1;
    }
    end
}

/// `w` unchanged, but on x86-64 kept in a register for the single-bit
/// update that follows. Left alone, LLVM folds `*w = *w | bit` into a
/// memory-destination `or` and builds `!bit` for a clear with a two-uop
/// `rol`, both of which cost throughput; with the load kept apart it picks a
/// one-uop register `bts` / `btr` and a plain store. The empty `asm!` emits
/// no instruction.
#[inline(always)]
pub(crate) fn in_register<W: Word>(w: W) -> W {
    #[cfg(target_arch = "x86_64")]
    if W::BITS == 64 {
        let mut v = w.to_u64();
        // SAFETY: an empty template; it only claims to read and write `v`.
        unsafe {
            core::arch::asm!("/* {v} */", v = inout(reg) v, options(pure, nomem, nostack, preserves_flags));
        }
        return W::from_u64(v);
    }
    w
}

/// On aarch64, bitmaps this short (in words) skip redundant single-bit stores, see
/// [`BitSlice::set`].
const TINY_WORDS: usize = 8;

/// Borrowed bitmap, an unsized view over `[W]`.
///
/// Get one with [`BitSlice::new`] / [`BitSlice::new_mut`] or by
/// dereferencing a [`Bitmap`](crate::Bitmap). Out of range reads return
/// zero unless the method says otherwise; nothing here allocates.
#[repr(transparent)]
pub struct BitSlice<W: Word = usize> {
    pub(crate) words: [W],
}

impl<W: Word> BitSlice<W> {
    /// The words without trailing zero words.
    #[inline]
    fn trimmed(&self) -> &[W] {
        &self.words[..nonzero_len(&self.words)]
    }

    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b1010u8, 0b1];
    /// let view = BitSlice::new(&words);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [1, 3, 8]);
    /// ```
    #[inline(always)]
    pub const fn new(words: &[W]) -> &Self {
        // SAFETY: repr(transparent) over [W]
        unsafe { &*(words as *const [W] as *const Self) }
    }

    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u32; 4];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.set(70);
    /// assert_eq!(words[2], 1 << 6);
    /// ```
    #[inline(always)]
    pub const fn new_mut(words: &mut [W]) -> &mut Self {
        // SAFETY: repr(transparent) over [W]
        unsafe { &mut *(words as *mut [W] as *mut Self) }
    }

    /// The words, lowest first: bit `i` is bit `i % W::BITS` of
    /// `as_words()[i / W::BITS]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b101u8, 0b1];
    /// assert_eq!(BitSlice::new(&words).as_words(), &[0b101, 0b1]);
    /// ```
    #[inline(always)]
    pub const fn as_words(&self) -> &[W] {
        &self.words
    }

    /// The words, mutably. Writing to them changes the bits directly; the
    /// length cannot change.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u8; 2];
    /// BitSlice::new_mut(&mut words).as_words_mut()[1] = 0b10;
    /// assert_eq!(BitSlice::new(&words).first_set(), Some(9));
    /// ```
    #[inline(always)]
    pub const fn as_words_mut(&mut self) -> &mut [W] {
        &mut self.words
    }

    /// The words' memory as bytes; in bit order on a little-endian target.
    #[cfg(all(target_endian = "little", any(has_alloc, feature = "serde")))]
    #[inline]
    pub(crate) fn as_le_bytes(&self) -> &[u8] {
        // SAFETY: plain integers viewed as bytes, and u8 needs no alignment.
        unsafe {
            core::slice::from_raw_parts(
                self.words.as_ptr().cast::<u8>(),
                core::mem::size_of_val(&self.words),
            )
        }
    }

    /// Length in words.
    #[inline(always)]
    pub const fn len_words(&self) -> usize {
        self.words.len()
    }

    /// Length in bits, a multiple of the word size.
    #[inline(always)]
    pub const fn len_bits(&self) -> usize {
        self.words.len() * W::BITS
    }

    /// Size in bytes.
    #[inline(always)]
    pub const fn bytes(&self) -> usize {
        self.words.len() * (W::BITS / 8)
    }

    /// Clear all bits, keep the length.
    #[inline]
    pub fn zero(&mut self) {
        self.words.fill(W::ZERO);
    }

    /// Is every bit zero?
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// assert!(BitSlice::new(&[0u64; 8]).is_zero());
    /// assert!(!BitSlice::new(&[0u64, 1]).is_zero());
    /// ```
    #[inline]
    pub fn is_zero(&self) -> bool {
        if self.words.len() < 8 {
            // 4 to 7 64-bit words on an AVX2 build: two overlapping 256-bit
            // loads and one test instead of a branch per word.
            #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
            if W::BITS == 64 && self.words.len() >= 4 {
                // SAFETY: at least four words, so both 32-byte loads are inside.
                return unsafe {
                    crate::arch::all_zero_4_to_8(
                        self.words.as_ptr().cast::<u64>(),
                        self.words.len(),
                    )
                };
            }
            // Short: a plain loop, chunking would only add setup.
            return self.words.iter().all(|&x| x == W::ZERO);
        }
        // Long: a loop with an early exit is not vectorized. OR eight words
        // with no branch (SIMD) and test once per chunk instead.
        let mut chunks = self.words.chunks_exact(8);
        for c in &mut chunks {
            let any = c[0] | c[1] | c[2] | c[3] | c[4] | c[5] | c[6] | c[7];
            if any != W::ZERO {
                return false;
            }
        }
        chunks.remainder().iter().all(|&x| x == W::ZERO)
    }

    /// Bit `i`. A bit past the end reads as zero.
    ///
    /// So `false` means either a clear bit or an index out of range, and you
    /// can't tell which. That is deliberate: a bitmap is zero-extended without
    /// end, and its length in words is not part of its value (equality ignores
    /// it, and `unset(i)` past the end does nothing). When you need to
    /// know, use [`BitSlice::try_get`], which returns `None` out of range, or
    /// [`BitSlice::get_strict`], which panics.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b10u64];
    /// let view = BitSlice::new(&words);
    /// assert!(view.get(1));
    /// assert!(!view.get(0));
    /// assert!(!view.get(1_000)); // same answer as a clear bit
    /// ```
    #[inline]
    pub fn get(&self, i: usize) -> bool {
        // x86-64: compare `i` with the length in bits rather than `i / BITS`
        // with the length in words. In a vectorized loop of reads (a masked
        // gather) the in-range mask then no longer waits for the shift; the
        // check is exact unless the length in bits overflows, which the
        // outer test sends to the general path.
        #[cfg(target_arch = "x86_64")]
        if self.words.len() <= usize::MAX / W::BITS {
            if i >= self.words.len() * W::BITS {
                return false;
            }
            // SAFETY: i < len * BITS, so i / BITS < len.
            let w = unsafe { *self.words.get_unchecked(i / W::BITS) };
            return (w >> (i % W::BITS)) & W::ONE != W::ZERO;
        }
        match self.words.get(i / W::BITS) {
            Some(&w) => (w >> (i % W::BITS)) & W::ONE != W::ZERO,
            None => false,
        }
    }

    /// Bit `i`, or `None` if `i` is out of range.
    ///
    /// Unlike [`BitSlice::get`], this tells a clear bit (`Some(false)`) from an
    /// index past the end (`None`).
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b10u64];
    /// let view = BitSlice::new(&words);
    /// assert_eq!(view.try_get(1), Some(true));
    /// assert_eq!(view.try_get(0), Some(false)); // a clear bit
    /// assert_eq!(view.try_get(64), None); // out of range
    /// ```
    #[inline]
    pub fn try_get(&self, i: usize) -> Option<bool> {
        let w = *self.words.get(i / W::BITS)?;
        Some((w >> (i % W::BITS)) & W::ONE != W::ZERO)
    }

    /// Bit `i`. Panics if `i` is out of range, where [`BitSlice::get`] would
    /// read it as zero. Use it when the size is fixed and an out-of-range
    /// index is a bug you want to hear about.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b10u64];
    /// assert!(BitSlice::new(&words).get_strict(1));
    /// ```
    ///
    /// ```should_panic
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b10u64];
    /// BitSlice::new(&words).get_strict(64); // only bits 0 to 63 exist
    /// ```
    #[inline]
    pub fn get_strict(&self, i: usize) -> bool {
        (self.words[i / W::BITS] >> (i % W::BITS)) & W::ONE != W::ZERO
    }

    /// Set bit `i` to one. Panics if out of range, never grows or trims.
    ///
    /// On a [`Bitmap`](crate::Bitmap) the growing
    /// [`Bitmap::set`](crate::Bitmap::set) takes precedence; borrow the bitmap
    /// as a `&mut BitSlice` to get this one.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u64; 2];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.set(3);
    /// view.set(100);
    /// view.unset(3);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [100]);
    /// ```
    #[inline]
    pub fn set(&mut self, i: usize) {
        let few = self.words.len() <= TINY_WORDS;
        let w = &mut self.words[i / W::BITS];
        let new = in_register(*w) | W::bit(i % W::BITS);
        // On aarch64, in a bitmap of a few words consecutive writes keep
        // hitting the same word, and every unconditional store makes the next
        // load of it wait (store-to-load forwarding): skipping the store when
        // nothing changes removes that chain. In a larger bitmap the outcome
        // is unpredictable and the branch costs more than the store saves; on
        // x86-64 the plain store is faster at every size.
        if cfg!(not(target_arch = "aarch64")) || !few || new != *w {
            *w = new;
        }
    }

    /// Clear bit `i`. Panics if out of range, never grows or trims.
    #[inline]
    pub fn unset(&mut self, i: usize) {
        let few = self.words.len() <= TINY_WORDS;
        let w = &mut self.words[i / W::BITS];
        let new = in_register(*w) & !W::bit(i % W::BITS);
        if cfg!(not(target_arch = "aarch64")) || !few || new != *w {
            *w = new;
        }
    }

    /// Set every bit yielded by `bits`. Panics at the first index out of
    /// range (bits before it stay set), never grows.
    ///
    /// The strict counterpart of [`Bitmap::set_many`](crate::Bitmap::set_many),
    /// and as fast as a loop of [`BitSlice::set`] on a borrowed view: the
    /// pointer and length stay in registers.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u64; 2];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.set_many_strict([3, 64, 127]);
    /// assert_eq!(view.iter_ones().collect::<Vec<_>>(), [3, 64, 127]);
    /// ```
    #[inline]
    pub fn set_many_strict(&mut self, bits: impl IntoIterator<Item = usize>) {
        for i in bits {
            self.set(i);
        }
    }

    /// Bits `i..i + n_bits` as an integer, `n_bits <= 64`. Bits beyond the
    /// end read as zero. Panics if `n_bits > 64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0xf000_0000_0000_0000u64, 0xff];
    /// let view = BitSlice::new(&words);
    /// // a 12-bit field starting at bit 60 straddles the two words
    /// assert_eq!(view.get_field(60, 12), 0xfff);
    /// ```
    #[inline]
    pub fn get_field(&self, i: usize, n_bits: usize) -> u64 {
        assert!(n_bits <= 64);

        // Little-endian fast path: bit `i` is bit `i % 8` of byte `i / 8`, so
        // one unaligned 8-byte load at that byte holds any field of up to 56
        // bits, whichever words it straddles. Only loads that lie fully inside
        // the bitmap qualify; the tail takes the general path below.
        #[cfg(target_endian = "little")]
        if n_bits <= 56 {
            let byte = i / 8;
            let total = core::mem::size_of_val(&self.words);
            if byte + 8 <= total {
                // SAFETY: bytes `byte..byte + 8` are inside the words (checked
                // above); read_unaligned has no alignment requirement and any
                // bytes are a valid u64.
                let v = unsafe {
                    self.words
                        .as_ptr()
                        .cast::<u8>()
                        .add(byte)
                        .cast::<u64>()
                        .read_unaligned()
                };
                return (v >> (i % 8)) & ((1u64 << n_bits) - 1);
            }
        }

        let i0 = i / W::BITS;
        let i1 = i % W::BITS;
        let l = self.words.len();

        if W::BITS >= 64 {
            // At most two words are involved.
            let mut result = W::ZERO;
            if i0 < l {
                result = (self.words[i0] >> i1) & W::low_mask(n_bits);
            }
            if i1 + n_bits > W::BITS && i0 + 1 < l {
                let n = n_bits - (W::BITS - i1);
                result = result | ((self.words[i0 + 1] & W::low_mask(n)) << (W::BITS - i1));
            }
            result.to_u64()
        } else {
            // Narrow words: gather the bits word by word.
            let mut result = 0u64;
            let mut got = 0;
            let mut pos = i;
            while got < n_bits {
                let wi = pos / W::BITS;
                if wi >= l {
                    break;
                }
                let off = pos % W::BITS;
                let take = (W::BITS - off).min(n_bits - got);
                let v = ((self.words[wi] >> off) & W::low_mask(take)).to_u64();
                result |= v << got;
                got += take;
                pos += take;
            }
            result
        }
    }

    /// Like [`BitSlice::get_field`] but the field must lie inside one word
    /// (only checked in debug builds). Panics if the word is out of range.
    #[inline]
    pub fn get_field_strict(&self, i: usize, n_bits: usize) -> u64 {
        debug_assert!(i % W::BITS + n_bits <= W::BITS);
        ((self.words[i / W::BITS] >> (i % W::BITS)) & W::low_mask(n_bits)).to_u64()
    }

    /// Store the low `n_bits` of `value` at bit `i`; the range must be in
    /// bounds. Never resizes or trims. Panics if out of range or
    /// `n_bits > 64`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let mut words = [0u64; 2];
    /// let view = BitSlice::new_mut(&mut words);
    /// view.set_field_strict(60, 0xabcd, 16);
    /// assert_eq!(view.get_field(60, 16), 0xabcd);
    /// ```
    #[inline]
    pub fn set_field_strict(&mut self, i: usize, value: u64, n_bits: usize) {
        assert!(n_bits <= 64);
        if n_bits == 0 {
            return;
        }
        let i0 = i / W::BITS;
        let i1 = i % W::BITS;

        if W::BITS >= 64 {
            let m = W::low_mask(n_bits);
            let v = W::from_u64(value) & m;
            let t = &mut self.words[i0];
            *t = (*t & !(m << i1)) | (v << i1);

            if i1 + n_bits > W::BITS {
                let done = W::BITS - i1; // < n_bits <= 64
                let m = W::low_mask(n_bits - done);
                let v = W::from_u64(value >> done) & m;
                let t = &mut self.words[i0 + 1];
                *t = (*t & !m) | v;
            }
        } else {
            let mut got = 0;
            let mut pos = i;
            while got < n_bits {
                let off = pos % W::BITS;
                let take = (W::BITS - off).min(n_bits - got);
                let m = W::low_mask(take);
                let v = W::from_u64(value >> got) & m;
                let t = &mut self.words[pos / W::BITS];
                *t = (*t & !(m << off)) | (v << off);
                got += take;
                pos += take;
            }
        }
    }

    /// Lowest set bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0u64, 0b100];
    /// let view = BitSlice::new(&words);
    /// assert_eq!(view.first_set(), Some(66));
    /// assert_eq!(BitSlice::<u64>::new(&[0, 0]).first_set(), None);
    /// ```
    #[inline]
    pub fn first_set(&self) -> Option<usize> {
        // On aarch64 the long-bitmap scan is kept out of line so that the
        // short case stays a few instructions in its caller. Elsewhere the
        // call costs more than it saves, so it is inlined.
        #[cfg(target_arch = "aarch64")]
        if self.words.len() >= 8 {
            return self.first_set_long();
        }
        // Short bitmap: the shortest possible scan, carrying the bit offset.
        #[cfg(not(target_arch = "aarch64"))]
        if self.words.len() >= 8 {
            let (p, x) = first_nonzero(&self.words)?;
            return Some(p * W::BITS + x.trailing_zeros() as usize);
        }
        // aarch64: test two words per branch with one paired load.
        #[cfg(target_arch = "aarch64")]
        {
            let mut off = 0;
            let mut pairs = self.words.chunks_exact(2);
            for c in &mut pairs {
                if (c[0] | c[1]) != W::ZERO {
                    let (x, skip) = if c[0] != W::ZERO {
                        (c[0], 0)
                    } else {
                        (c[1], W::BITS)
                    };
                    return Some(off + skip + x.trailing_zeros() as usize);
                }
                off += 2 * W::BITS;
            }
            match pairs.remainder() {
                [x] if *x != W::ZERO => Some(off + x.trailing_zeros() as usize),
                _ => None,
            }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let mut off = 0;
            // 64-bit words on an AVX2 build: test four words with one load.
            #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
            let rest = if W::BITS == 64 && self.words.len() >= 4 {
                // SAFETY: at least four words (32 bytes) are readable.
                if let Some(lane) = unsafe {
                    crate::arch::first_nonzero_lane_of_4(self.words.as_ptr().cast::<u64>())
                } {
                    return Some(lane * 64 + self.words[lane].trailing_zeros() as usize);
                }
                off = 4 * W::BITS;
                &self.words[4..]
            } else {
                &self.words[..]
            };
            #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
            let rest = &self.words[..];
            for &x in rest {
                if x != W::ZERO {
                    return Some(off + x.trailing_zeros() as usize);
                }
                off += W::BITS;
            }
            None
        }
    }

    /// `first_set` for 8 or more words, scanning eight words per step.
    #[cfg(target_arch = "aarch64")]
    #[cold]
    #[inline(never)]
    fn first_set_long(&self) -> Option<usize> {
        let (p, x) = first_nonzero(&self.words)?;
        Some(p * W::BITS + x.trailing_zeros() as usize)
    }

    /// Highest set bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b1u64, 0b100, 0];
    /// assert_eq!(BitSlice::new(&words).last_set(), Some(66));
    /// ```
    #[inline]
    pub fn last_set(&self) -> Option<usize> {
        let (p, x) = if self.words.len() < 8 {
            // short bitmap: plain reverse scan
            self.words
                .iter()
                .enumerate()
                .rev()
                .find(|(_, x)| **x != W::ZERO)
                .map(|(p, &x)| (p, x))?
        } else {
            let p = nonzero_len(&self.words).checked_sub(1)?;
            // SAFETY: nonzero_len <= len, so p < len.
            (p, unsafe { *self.words.get_unchecked(p) })
        };
        Some(p * W::BITS + (W::BITS - 1 - x.leading_zeros() as usize))
    }

    /// Lowest clear bit; the bit length if every bit is set.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// assert_eq!(BitSlice::new(&[0b0111u8]).first_clear(), 3);
    /// // every bit set: the answer is the bit length
    /// assert_eq!(BitSlice::new(&[0xffu8]).first_clear(), 8);
    /// ```
    #[inline]
    pub fn first_clear(&self) -> usize {
        match first_not_ones(&self.words) {
            Some((p, x)) => p * W::BITS + (!x).trailing_zeros() as usize,
            None => self.len_bits(),
        }
    }

    /// Number of set bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b1011u64, 0xff];
    /// assert_eq!(BitSlice::new(&words).count_set_bits(), 11);
    /// ```
    #[inline]
    pub fn count_set_bits(&self) -> usize {
        #[cfg(target_arch = "aarch64")]
        if self.words.len() >= 8 {
            // SAFETY: the words are plain integers: viewing their memory as
            // bytes is always valid, and u8 needs no alignment.
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    self.words.as_ptr().cast::<u8>(),
                    core::mem::size_of_val(&self.words),
                )
            };
            return crate::arch::popcount_bytes(bytes);
        }
        self.words.iter().map(|x| x.count_ones() as usize).sum()
    }

    /// First set bit at or after `i`.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b0110_0000u64, 1];
    /// let view = BitSlice::new(&words);
    /// assert_eq!(view.next_set(0), Some(5));
    /// assert_eq!(view.next_set(7), Some(64)); // starts looking at bit 7 itself
    /// assert_eq!(view.next_set(65), None);
    /// ```
    #[inline]
    pub fn next_set(&self, i: usize) -> Option<usize> {
        let i0 = i / W::BITS;
        let i1 = i % W::BITS;
        let w = &self.words;
        if i0 >= w.len() {
            return None;
        }
        let t = (w[i0] >> i1) << i1;
        if t != W::ZERO {
            return Some(i0 * W::BITS + t.trailing_zeros() as usize);
        }
        let (p, x) = first_nonzero(&w[i0 + 1..])?;
        Some((i0 + 1 + p) * W::BITS + x.trailing_zeros() as usize)
    }

    /// First clear bit at or after `i`. Bits past the end are clear, so
    /// this always succeeds.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let view = BitSlice::new(&[0b0111_0000u8]);
    /// assert_eq!(view.next_clear(4), 7);
    /// assert_eq!(view.next_clear(100), 100); // past the end every bit is clear
    /// ```
    #[inline]
    pub fn next_clear(&self, i: usize) -> usize {
        let i0 = i / W::BITS;
        let i1 = i % W::BITS;
        let w = &self.words;
        if i0 >= w.len() {
            return i;
        }
        let t = (!w[i0] >> i1) << i1;
        if t != W::ZERO {
            return i0 * W::BITS + t.trailing_zeros() as usize;
        }
        match first_not_ones(&w[i0 + 1..]) {
            Some((p, x)) => (i0 + 1 + p) * W::BITS + (!x).trailing_zeros() as usize,
            None => w.len() * W::BITS,
        }
    }

    /// Iterate over set bits in increasing order.
    ///
    /// # Examples
    ///
    /// ```
    /// use vbitmap::BitSlice;
    ///
    /// let words = [0b0110_0000u64, 1];
    /// let bits: Vec<usize> = BitSlice::new(&words).iter_ones().collect();
    /// assert_eq!(bits, [5, 6, 64]);
    /// ```
    #[inline]
    pub fn iter_ones(&self) -> Ones<'_, W> {
        Ones {
            words: &self.words,
            idx: 0,
            cur: self.words.first().copied().unwrap_or(W::ZERO),
        }
    }
}

/// `view[i]` is [`BitSlice::get`]: `false` past the end, no panic.
///
/// # Examples
///
/// ```
/// use vbitmap::BitSlice;
///
/// let view = BitSlice::new(&[0b10u8]);
/// assert!(view[1]);
/// assert!(!view[0]);
/// assert!(!view[100]);
/// ```
impl<W: Word> core::ops::Index<usize> for BitSlice<W> {
    type Output = bool;
    #[inline]
    fn index(&self, i: usize) -> &bool {
        if self.get(i) { &true } else { &false }
    }
}

impl<'a, W: Word> IntoIterator for &'a BitSlice<W> {
    type Item = usize;
    type IntoIter = Ones<'a, W>;
    #[inline]
    fn into_iter(self) -> Ones<'a, W> {
        self.iter_ones()
    }
}

impl<W: Word> AsRef<BitSlice<W>> for [W] {
    #[inline]
    fn as_ref(&self) -> &BitSlice<W> {
        BitSlice::new(self)
    }
}

/// Two bitmaps are equal when they have the same set bits, whatever their
/// length in words: trailing zero words are ignored.
impl<W: Word> PartialEq for BitSlice<W> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.trimmed() == other.trimmed()
    }
}

impl<W: Word> Eq for BitSlice<W> {}

impl<W: Word> core::hash::Hash for BitSlice<W> {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.trimmed().hash(state);
    }
}
