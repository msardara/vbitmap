// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fast growable bitmap over generic machine words.
//!
//! * [`Bitmap<W>`] owns its words (aligned to a 64-byte cache line) and grows
//!   on demand. `W` defaults to `usize`, the native machine word.
//! * [`BitSlice<W>`] is a view over a slice of words. The queries and the
//!   updates that don't resize are its methods. A `Bitmap` derefs to one, and
//!   you can build one over a `&[W]`, an array or a buffer you don't own,
//!   without copying.
//! * [`Word`] is implemented for `u8` to `u128` and `usize`. Everything is
//!   monomorphized and `#[inline]`, so `Bitmap<u64>` compiles to what you would
//!   write by hand for `u64` words.
//!
//! Bit `i` is stored in word `i / W::BITS`, at position `i % W::BITS`, least
//! significant bit first.
//!
//! # Quick start
//!
//! [`NativeBitmap`] is `Bitmap<usize>`: the machine's native word, and the
//! right choice unless you need a fixed layout.
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let mut flags = NativeBitmap::new();
//! flags.set(3);                  // grows as needed
//! flags.set(70);
//! flags.set_region(100, true, 5); // bits 100 to 104
//! flags.unset(3);
//!
//! assert!(flags.get(70));
//! assert!(!flags.get(3));
//! assert!(!flags.get(1_000_000)); // out of range reads as zero
//! assert_eq!(flags.count_set_bits(), 6);
//! assert_eq!(flags.first_set(), Some(70));
//! assert_eq!(flags.last_set(), Some(104));
//! assert_eq!(flags.next_set(71), Some(100));
//! assert_eq!(flags.iter_ones().collect::<Vec<_>>(), [70, 100, 101, 102, 103, 104]);
//! # }
//! ```
//!
//! # Combining bitmaps
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let a: NativeBitmap = [1usize, 2, 70, 200].into_iter().collect();
//! let b: NativeBitmap = [2usize, 70, 71].into_iter().collect();
//!
//! // In place, or as a new bitmap:
//! let mut x = a.clone();
//! x.and(&b);
//! assert_eq!(x, a.dup_and(&b));
//!
//! // `dst = a op b` in one pass over memory, which is faster than cloning `a`
//! // and applying the in-place operator:
//! let mut dst = NativeBitmap::new();
//! dst.or_from(&a, &b);
//! assert_eq!(dst.iter_ones().collect::<Vec<_>>(), [1, 2, 70, 71, 200]);
//! dst.xor_from(&a, &b);
//! assert_eq!(dst.iter_ones().collect::<Vec<_>>(), [1, 71, 200]);
//! # }
//! ```
//!
//! # Reading and writing text
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! // Range lists ("0-3,5-7,11"), as in CPU affinity masks:
//! let cpus = NativeBitmap::from_list("0-3,5-7,11").unwrap();
//! assert_eq!(cpus.format_list(), "0-3, 5-7, 11");
//! // Hexadecimal masks, most significant digit first:
//! assert_eq!(cpus.format_hex(), "8ef");
//! assert_eq!(NativeBitmap::from_hex_mask("0x8ef").unwrap(), cpus);
//! # }
//! ```
//!
//! # Fields of several bits
//!
//! A bitmap can also hold small integers, up to 64 bits wide, at any bit
//! offset, even across a word boundary:
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let mut b = NativeBitmap::new();
//! b.set_field(60, 0xabcd, 16);
//! assert_eq!(b.get_field(60, 16), 0xabcd);
//! // bits 60 to 63 hold the low nibble 0xd; bits 64 to 71 hold the next 8 bits
//! assert_eq!(b.get_field(64, 8), 0xbc);
//! # }
//! ```
//!
//! # Borrowed views, with no allocator
//!
//! [`BitSlice`] works on memory you already have (an array, a `static`, a
//! buffer from elsewhere) and never allocates, so it is available even with
//! the `no_std` feature alone:
//!
//! ```
//! use vbitmap::BitSlice;
//!
//! let mut words = [0u64; 4]; // 256 flags
//! let flags = BitSlice::new_mut(&mut words);
//! flags.set(3);   // panics if out of range, never grows
//! flags.set(200);
//!
//! assert_eq!(flags.count_set_bits(), 2);
//! assert_eq!(flags.first_set(), Some(3));
//! assert_eq!(flags.iter_ones().collect::<Vec<_>>(), [3, 200]);
//! ```
//!
//! # Many bits at once
//!
//! To set many bits, use [`Bitmap::set_many`]: one call, grows as needed, and
//! faster than a loop of `set` (each `set` through `&mut Bitmap` reloads the
//! bitmap's pointer and length).
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let mut b = NativeBitmap::new();
//! b.set_many((0..1024).step_by(3));
//! assert_eq!(b.count_set_bits(), 342);
//! # }
//! ```
//!
//! If the size is fixed and out-of-range writes should panic, use
//! [`BitSlice::set_many_strict`]. It never grows:
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let mut b = NativeBitmap::alloc(1024);
//! b.set_many_strict((0..1024).step_by(3)); // panics if an index is out of range
//! assert_eq!(b.count_set_bits(), 342);
//! # }
//! ```
//!
//! # Use cases
//!
//! ## Which slots of a pool are in use
//!
//! Objects live in a table and are named by their index. A bitmap says which
//! indices are taken: `first_clear` finds the lowest free slot, `set` takes it
//! (growing the bitmap when the pool is full), `unset` gives it back, and
//! `iter_ones` visits the live objects without touching the free ones. The
//! bitmap costs one bit per slot, so even a sparse pool is cheap to scan.
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! struct Pool<T> {
//!     slots: Vec<Option<T>>,
//!     used: NativeBitmap,
//! }
//!
//! impl<T> Pool<T> {
//!     fn new() -> Self {
//!         Pool { slots: Vec::new(), used: NativeBitmap::new() }
//!     }
//!
//!     /// Store `value` in the lowest free slot and return its index.
//!     fn insert(&mut self, value: T) -> usize {
//!         let i = self.used.first_clear(); // the bit length when full
//!         self.used.set(i); // grows the bitmap when needed
//!         if i == self.slots.len() {
//!             self.slots.push(Some(value));
//!         } else {
//!             self.slots[i] = Some(value);
//!         }
//!         i
//!     }
//!
//!     fn remove(&mut self, i: usize) -> Option<T> {
//!         self.used.unset(i);
//!         self.slots.get_mut(i)?.take()
//!     }
//!
//!     /// The live objects, in index order.
//!     fn iter(&self) -> impl Iterator<Item = (usize, &T)> {
//!         self.used.iter_ones().filter_map(|i| Some((i, self.slots[i].as_ref()?)))
//!     }
//! }
//!
//! let mut pool = Pool::new();
//! let a = pool.insert("a");
//! let b = pool.insert("b");
//! let c = pool.insert("c");
//! assert_eq!((a, b, c), (0, 1, 2));
//!
//! pool.remove(b);
//! assert_eq!(pool.insert("d"), 1); // the freed slot is reused first
//! pool.remove(a);
//! pool.remove(c);
//! assert_eq!(pool.iter().collect::<Vec<_>>(), [(1, &"d")]);
//! # }
//! ```
//!
//! ## CPU sets and other masks
//!
//! A set of small integers such as CPU numbers, interrupt lines or user ids is
//! a bitmap. The usual list notation (`0-3,8`) parses and prints directly, and
//! the set algebra is one call each:
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let allowed = NativeBitmap::from_list("0-7,16-23").unwrap();
//! let online = NativeBitmap::from_list("0-3,6-19").unwrap();
//!
//! let mut usable = allowed.clone();
//! usable.and(&online); // allowed and online
//! assert_eq!(usable.format_list(), "0-3, 6-7, 16-19");
//! assert_eq!(usable.count_set_bits(), 10);
//!
//! // Round-robin: the next usable CPU after the one used last.
//! let last = 7;
//! assert_eq!(usable.next_set(last + 1), Some(16));
//! # }
//! ```
//!
//! ## Dirty tracking
//!
//! Mark what changed, then visit only that. `set_region` marks a run of pages
//! in a few word writes, and `zero` clears everything for the next round:
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::NativeBitmap;
//!
//! let mut dirty = NativeBitmap::alloc(1024); // one bit per page
//! dirty.set(3); // a write to page 3
//! dirty.set_region(100, true, 8); // a write spanning pages 100 to 107
//!
//! let mut flushed = Vec::new();
//! for page in dirty.iter_ones() {
//!     flushed.push(page); // write the page out
//! }
//! assert_eq!(flushed, [3, 100, 101, 102, 103, 104, 105, 106, 107]);
//!
//! dirty.zero(); // all clean again, the allocation is kept
//! assert!(dirty.is_zero());
//! # }
//! ```
//!
//! # Word types
//!
//! `Bitmap<W>` takes `u8`, `u16`, `u32`, `u64`, `u128` or `usize` words.
//! [`Bitmap64`] is `Bitmap<u64>` for a layout that is the same on every target:
//!
//! ```
//! # #[cfg(any(not(feature = "no_std"), feature = "alloc"))] {
//! use vbitmap::Bitmap64;
//!
//! let b = Bitmap64::from_words(vec![0b101, 1]);
//! assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 2, 64]);
//! assert_eq!(b.as_words(), &[0b101, 1]);
//! # }
//! ```
//!
//! # Length and trailing zeros
//!
//! A bitmap is zero-extended without end: every bit past the stored words reads
//! as zero, and clearing a bit there does nothing. So `get` can't tell a clear
//! bit from an out-of-range index; `try_get` returns `None` for the latter and
//! `get_strict` panics.
//!
//! `set` never shrinks a bitmap; the bulk operations that can clear words
//! (`and`, `andnot`, `xor`) drop trailing zero words, and `trim` does it on
//! demand. Equality, hashing and every query ignore trailing zero words, so the
//! length in words never changes a result.
//!
//! # Performance
//!
//! How many times faster `vbitmap` is than the best of the other dense bitmap
//! crates (`fixedbitset`, `bit-vec`, `bit-set`, `bitvec`, `bv`), as the range
//! over bitmaps of 256, 4096 and 262144 bits. Above 1x means `vbitmap` is
//! faster.
//!
//! | operation | Apple M4 (arm64) | Xeon 6737P (x86-64) |
//! |---|---|---|
//! | first set bit, bitmap otherwise empty | 0.95x to 4.8x | 1.3x to 1.9x |
//! | is it all zero | 1.4x to 2.0x | 1.3x to 1.8x |
//! | population count | 1.1x to 1.6x | 1.0x |
//! | `dst = a & b`, `a \| b`, `a ^ b` (`and_from`, ...) | 1.3x to 1.9x | 1.3x to 1.8x |
//! | set or clear a range | 1.2x to 1.5x | 1.6x to 3.1x |
//! | read a 24-bit field | 1.7x to 2.3x | 1.1x |
//! | iterate set bits, dense | 1.2x to 1.6x | 1.0x to 1.5x |
//! | iterate set bits, sparse | 1.1x to 1.3x | 1.0x to 1.4x |
//! | single-bit writes (`set`, `unset` on a `BitSlice`) | 1.2x to 1.4x | 1.2x to 1.5x |
//! | random single-bit reads | 1.0x | 1.0x |
//!
//! `vbitmap` is at least 5% ahead in 60 of the 72 measurements behind this table
//! and within about 5% in the other 12. They come from one build on two
//! machines, so expect different numbers on yours; a few fast paths exist for
//! one architecture only (NEON on aarch64, AVX2 on x86-64). The full tables and
//! the method are in
//! [BENCHMARKS.md](https://github.com/msardara/vbitmap/blob/main/BENCHMARKS.md),
//! and `just compare` reruns them with
//! [`benches/compare.rs`](https://github.com/msardara/vbitmap/blob/main/benches/compare.rs).
//!
//! # Features
//!
//! By default the crate uses `std`. The `no_std` feature makes it
//! `#![no_std]`; add the `alloc` feature to keep [`Bitmap`], string formatting
//! and parsing. With `no_std` alone only the borrowed [`BitSlice`] API remains.
//!
//! `alloc` is the toolchain's own `alloc` crate, nothing to install. On a
//! target without one, your binary must provide a `#[global_allocator]`.

#![cfg_attr(feature = "no_std", no_std)]
#![warn(missing_docs)]
// `Bitmap` is not built (and so not linkable) with `no_std` alone.
#![cfg_attr(not(has_alloc), allow(rustdoc::broken_intra_doc_links))]

#[cfg(has_alloc)]
extern crate alloc;

// Run the README's code blocks as doctests, so they cannot drift.
#[cfg(all(doctest, has_alloc))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod algebra;
mod arch;
mod array;
#[cfg(has_alloc)]
mod bitmap;
mod bits;
mod iter;
#[cfg(feature = "serde")]
mod serde;
#[cfg(has_alloc)]
mod shape;
mod shift;
mod slice;
#[cfg(has_alloc)]
mod storage;
mod text;
mod word;

pub use array::ArrayBitmap;
#[cfg(has_alloc)]
pub use bitmap::Bitmap;
pub use iter::{Ones, Zeros};
pub use slice::BitSlice;
pub use text::ParseError;
pub use word::Word;

/// [`Bitmap`] of native machine words (`usize`). Unlike `Bitmap`, the bare
/// `NativeBitmap::new()` needs no turbofish.
#[cfg(has_alloc)]
pub type NativeBitmap = Bitmap<usize>;
/// [`BitSlice`] of native machine words (`usize`).
pub type NativeBitSlice = BitSlice<usize>;
/// [`Bitmap`] of 64-bit words.
#[cfg(has_alloc)]
pub type Bitmap64 = Bitmap<u64>;
/// [`BitSlice`] of 64-bit words.
pub type BitSlice64 = BitSlice<u64>;
