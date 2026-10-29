// SPDX-License-Identifier: MIT OR Apache-2.0
//! Word storage aligned to a cache line.
//!
//! A `Vec<W>` is only aligned to `align_of::<W>()` (the allocator usually
//! gives 16 bytes), so a 32 or 64 byte vector load or store regularly straddles
//! two cache lines. Bulk operations over a bitmap are about 15% faster when
//! the words start on a cache line, so the owned [`Bitmap`](crate::Bitmap)
//! keeps its words in 64-byte aligned blocks.

use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::slice;

use crate::Word;

const ALIGN: usize = 64;

/// One cache line of raw bytes. Every bit pattern is a valid integer, so the
/// bytes can be viewed as words of any `Word` type.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
struct Block([u8; ALIGN]);

const ZERO_BLOCK: Block = Block([0; ALIGN]);

/// A vector of words whose first word is 64-byte aligned.
///
/// Invariant: `len <= blocks.len() * WORDS_PER_BLOCK`.
#[derive(Clone)]
pub(crate) struct AlignedWords<W: Word> {
    blocks: Vec<Block>,
    len: usize,
    _word: PhantomData<W>,
}

impl<W: Word> Default for AlignedWords<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Word> AlignedWords<W> {
    const WORDS_PER_BLOCK: usize = ALIGN / (W::BITS / 8);

    fn blocks_for(words: usize) -> usize {
        words.div_ceil(Self::WORDS_PER_BLOCK)
    }

    pub(crate) const fn new() -> Self {
        Self {
            blocks: Vec::new(),
            len: 0,
            _word: PhantomData,
        }
    }

    /// `len` zero words.
    pub(crate) fn zeroed(len: usize) -> Self {
        let mut s = Self::new();
        s.blocks.resize(Self::blocks_for(len), ZERO_BLOCK);
        s.len = len;
        s
    }

    /// Empty, with room for `words` words.
    pub(crate) fn with_capacity(words: usize) -> Self {
        Self {
            blocks: Vec::with_capacity(Self::blocks_for(words)),
            len: 0,
            _word: PhantomData,
        }
    }

    pub(crate) fn from_slice(src: &[W]) -> Self {
        let mut s = Self::new();
        s.extend_from_slice(src);
        s
    }

    /// Capacity in words, before a reallocation.
    pub(crate) fn capacity(&self) -> usize {
        self.blocks.capacity() * Self::WORDS_PER_BLOCK
    }

    /// Make room for `words` words in total without reallocating.
    pub(crate) fn reserve_total(&mut self, words: usize) {
        let need = Self::blocks_for(words);
        if need > self.blocks.len() {
            self.blocks.reserve(need - self.blocks.len());
        }
    }

    /// Release the capacity beyond `len` words.
    pub(crate) fn shrink_to_fit(&mut self) {
        self.blocks.truncate(Self::blocks_for(self.len));
        self.blocks.shrink_to_fit();
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }

    /// Grow to `new_len` words (never shrinks), filling the new words with
    /// zero.
    pub(crate) fn resize_zeroed(&mut self, new_len: usize) {
        debug_assert!(new_len >= self.len, "use truncate to shrink");
        self.ensure_blocks(new_len);
        let old = self.len;
        self.len = new_len;
        // Words past the old length can hold stale data from before a
        // truncate or clear.
        self.as_mut_slice()[old..].fill(W::ZERO);
    }

    /// Append `src`. The copy itself is a small inlinable fast path; making
    /// room is out of line and cold.
    #[inline]
    pub(crate) fn extend_from_slice(&mut self, src: &[W]) {
        let old = self.len;
        let new_len = old + src.len();
        if Self::blocks_for(new_len) > self.blocks.len() {
            self.grow_blocks(new_len);
        }
        // SAFETY: the blocks cover `new_len` words (checked or grown above),
        // `src` cannot overlap our own storage because we hold `&mut self`,
        // and the destination is aligned for `W`.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr(),
                self.blocks.as_mut_ptr().cast::<W>().add(old),
                src.len(),
            );
        }
        self.len = new_len;
    }

    /// Set the length to `len` and let `f` fill every word. The words handed
    /// to `f` hold stale (but valid) data from earlier use, so `f` must write
    /// all of them. Avoids zero-filling words that are about to be overwritten.
    #[inline]
    pub(crate) fn overwrite(&mut self, len: usize, f: impl FnOnce(&mut [W])) {
        self.ensure_blocks(len);
        self.len = len;
        f(self.as_mut_slice());
    }

    pub(crate) fn to_vec(&self) -> Vec<W> {
        self.as_slice().to_vec()
    }

    fn ensure_blocks(&mut self, words: usize) {
        if Self::blocks_for(words) > self.blocks.len() {
            self.grow_blocks(words);
        }
    }

    #[cold]
    #[inline(never)]
    fn grow_blocks(&mut self, words: usize) {
        self.blocks.resize(Self::blocks_for(words), ZERO_BLOCK);
    }

    pub(crate) fn as_slice(&self) -> &[W] {
        // SAFETY: `blocks` holds at least `len` words (type invariant), is
        // aligned to 64 >= align_of::<W>(), and any bytes are a valid integer.
        unsafe { slice::from_raw_parts(self.blocks.as_ptr().cast::<W>(), self.len) }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [W] {
        // SAFETY: as in `as_slice`, and we hold `&mut self`.
        unsafe { slice::from_raw_parts_mut(self.blocks.as_mut_ptr().cast::<W>(), self.len) }
    }
}

impl<W: Word> Deref for AlignedWords<W> {
    type Target = [W];
    #[inline(always)]
    fn deref(&self) -> &[W] {
        self.as_slice()
    }
}

impl<W: Word> DerefMut for AlignedWords<W> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut [W] {
        self.as_mut_slice()
    }
}
