// SPDX-License-Identifier: MIT OR Apache-2.0
//! Logical operators between bitmaps, with a single-bit immediate, and
//! equality.
//!
//! Tip: unless you need to know the word size, use `NativeBitmap`
//! (`Bitmap<usize>`): `usize` is the machine's native word (64 bits on 64-bit
//! targets, 32 on 32-bit ones), so every step
//! handles a full register. Where an example asserts something that depends
//! on the word size (word counts, raw words) it uses `Bitmap64`, which aliases
//! `Bitmap<u64>`.
//!
//! Run with `cargo run --example logic`.

#[cfg(has_alloc)]
fn main() {
    use std::collections::HashSet;

    use vbitmap::{BitSlice, NativeBitmap};

    // `NativeBitmap` aliases `Bitmap<usize>`; nothing here depends on the word size.
    let bm = |bits: &[usize]| -> NativeBitmap { bits.iter().copied().collect() };
    let a = bm(&[1, 2, 70, 200]);
    let b = bm(&[2, 70, 71]);

    // In place: `a op= b`. The shorter operand is zero extended.
    let mut x = a.clone();
    x.and(&b);
    assert_eq!(x, bm(&[2, 70]));
    let mut x = a.clone();
    x.andnot(&b);
    assert_eq!(x, bm(&[1, 200]));
    let mut x = a.clone();
    x.or(&b);
    assert_eq!(x, bm(&[1, 2, 70, 71, 200]));
    let mut x = a.clone();
    x.xor(&b);
    assert_eq!(x, bm(&[1, 71, 200]));

    // As new bitmaps.
    assert_eq!(a.dup_and(&b), bm(&[2, 70]));
    assert_eq!(a.dup_andnot(&b), bm(&[1, 200]));
    assert_eq!(a.dup_or(&b), bm(&[1, 2, 70, 71, 200]));
    assert_eq!(a.dup_xor(&b), bm(&[1, 71, 200]));

    // Fused: `dst = a op b` in one pass over memory, discarding what `dst` held.
    // Cloning `a` and then applying the in-place form touches every word twice;
    // these write each result word once, and are about 1.6x faster on large
    // bitmaps.
    let mut dst = NativeBitmap::new();
    dst.and_from(&a, &b);
    assert_eq!(dst, bm(&[2, 70]));
    dst.andnot_from(&a, &b);
    assert_eq!(dst, bm(&[1, 200]));
    dst.or_from(&a, &b);
    assert_eq!(dst, bm(&[1, 2, 70, 71, 200]));
    dst.xor_from(&a, &b);
    assert_eq!(dst, bm(&[1, 71, 200]));

    // Operator forms take a `&BitSlice`, so a plain word slice works too.
    let words = [0b100usize]; // a view must use the same word type as the bitmap
    let mut x = bm(&[0]);
    x |= BitSlice::new(&words);
    assert_eq!(x, bm(&[0, 2]));
    x &= &b; // `&Bitmap` derefs to `&BitSlice`
    assert_eq!(x, bm(&[2]));
    let mut x = a.clone();
    x ^= &b;
    assert_eq!(x, bm(&[1, 71, 200]));

    // Immediates act on the whole word `i / BITS` with the mask `1 << (i % BITS)`.
    let mut w = bm(&[3]);
    w.ori(5); // word |= 1 << 5
    assert_eq!(w, bm(&[3, 5]));
    w.xori(3); // word ^= 1 << 3
    assert_eq!(w, bm(&[5]));
    w.andnoti(5); // word &= !(1 << 5): word becomes zero, so it is trimmed
    assert_eq!(w.len_words(), 0);
    w.ori(5);
    w.andi(5); // word &= 1 << 5
    assert_eq!(w, bm(&[5]));

    // The `_notrim` variants leave a zero word behind instead of trimming.
    let mut w = bm(&[5]);
    w.andnoti_notrim(5);
    assert_eq!(w.len_words(), 1);
    assert!(w.is_zero());
    w.xori_notrim(5);
    w.andi_notrim(6); // word &= 1 << 6 clears bit 5
    assert_eq!(w.len_words(), 1);
    w.trim();
    assert_eq!(w.len_words(), 0);

    // Equality and hashing ignore trailing zero words.
    let mut padded = NativeBitmap::alloc(1024);
    padded.set(7);
    assert_eq!(padded, bm(&[7]));
    let set: HashSet<NativeBitmap> = [padded, bm(&[7])].into_iter().collect();
    assert_eq!(set.len(), 1);

    println!("logic: ok");
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
