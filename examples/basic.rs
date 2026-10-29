// SPDX-License-Identifier: MIT OR Apache-2.0
//! Construction, storage and single-bit access.
//!
//! Tip: unless you need to know the word size, use `NativeBitmap`
//! (`Bitmap<usize>`): `usize` is the machine's native word (64 bits on 64-bit
//! targets, 32 on 32-bit ones), so every step
//! handles a full register. Where an example asserts something that depends
//! on the word size (word counts, raw words) it uses `Bitmap64`, which aliases
//! `Bitmap<u64>`.
//!
//! Run with `cargo run --example basic`.

#[cfg(has_alloc)]
fn main() {
    use vbitmap::{BitSlice, Bitmap, Bitmap64, NativeBitmap};

    // `NativeBitmap` aliases `Bitmap<usize>`: native machine words.
    let mut b = NativeBitmap::new(); // empty
    assert!(b.is_zero() && b.len_words() == 0);

    // `Bitmap<W>` picks the word type. `Bitmap64` aliases `Bitmap<u64>`: 64-bit
    // words on every target.
    let a = Bitmap64::alloc(130); // zeroed, covers 130 bits
    assert_eq!((a.len_words(), a.len_bits(), a.bytes()), (3, 192, 24));
    let _spare = Bitmap64::with_capacity_bits(1024); // empty, room reserved
    let from_words = Bitmap64::from_words(vec![0b101, 1]); // bit 0 = LSB of word 0
    assert_eq!(from_words.as_words(), &[0b101, 1]);
    let from_vec: Bitmap<u8> = vec![0xf0u8].into(); // `From<Vec<W>>`, here with `u8` words
    assert_eq!(from_vec.count_set_bits(), 4);
    let collected: NativeBitmap = [1usize, 5, 9].into_iter().collect(); // `FromIterator`
    assert_eq!(collected.format_list(), "1, 5, 9");
    let _default = NativeBitmap::default();
    assert_eq!(from_words.clone().into_words(), vec![0b101, 1]);

    b.set(3); // grows as needed
    b.set(70);
    assert!(b.get(3) && b.get(70) && !b.get(4));
    assert!(!b.get(1_000_000)); // out of range reads as zero, no panic
    b.unset(500); // clearing beyond the end allocates nothing
    assert_eq!(b.len_words(), 2.max(70 / usize::BITS as usize + 1));

    b.unset(70); // clearing never shrinks the bitmap...
    let before = b.len_words();
    b.trim(); // ...`trim` drops the trailing zero words
    assert!(b.len_words() < before || before == 1);

    // `set` grows, `unset` never does (bits past the end are already zero).
    let mut flags = NativeBitmap::new();
    flags.set(9);
    flags.set(300);
    flags.unset(9);
    flags.unset(1_000_000);
    assert_eq!(flags.format_list(), "300");

    // Through a borrowed `BitSlice`, `set` / `unset` never grow and panic when
    // out of range: the form to use in a hot loop on a bitmap of known size.
    let view: &mut BitSlice = &mut flags;
    view.set(10);
    view.unset(300);
    assert_eq!(view.format_list(), "10");
    assert!(view.try_get(1_000_000).is_none());
    assert_eq!(view.try_get(10), Some(true));

    // `validate(200)` makes 4 words only with 64-bit words, so use `Bitmap64`.
    let mut c = Bitmap64::new();
    c.validate(200); // make sure at least 200 bits exist
    assert_eq!(c.len_words(), 4);
    c.as_words_mut()[0] = 0xff;
    assert_eq!(c.count_set_bits(), 8);
    c.zero(); // clear every bit, keep the length
    assert!(c.is_zero() && c.len_words() == 4);
    c.trim(); // drop trailing zero words
    assert_eq!(c.len_words(), 0);
    c.set(130);
    assert!(!c.will_expand(100)); // capacity already covers it
    assert!(c.will_expand(100_000));
    c.clear(); // drop all words, keep the allocation
    assert_eq!(c.len_words(), 0);
    let src = Bitmap64::from_words(vec![1, 2, 3]);
    c.copy_from(&src); // reuse the allocation
    assert_eq!(c, src);

    // Equality ignores trailing zero words.
    let mut padded = NativeBitmap::alloc(1024);
    padded.set(0);
    assert_eq!(padded, [0usize].into_iter().collect::<NativeBitmap>());

    println!("basic: ok");
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
