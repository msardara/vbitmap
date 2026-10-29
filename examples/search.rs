// SPDX-License-Identifier: MIT OR Apache-2.0
//! Searching and iterating over set bits.
//!
//! Tip: unless you need to know the word size, use `NativeBitmap`
//! (`Bitmap<usize>`): `usize` is the machine's native word (64 bits on 64-bit
//! targets, 32 on 32-bit ones), so every step
//! handles a full register. Where an example asserts something that depends
//! on the word size (word counts, raw words) it uses `Bitmap64`, which aliases
//! `Bitmap<u64>`.
//!
//! Run with `cargo run --example search`.

#[cfg(has_alloc)]
fn main() {
    use vbitmap::{Bitmap64, NativeBitmap};

    // `NativeBitmap` aliases `Bitmap<usize>`; none of this depends on the word size.
    let b: NativeBitmap = [2usize, 3, 4, 64, 65, 200].into_iter().collect();

    assert_eq!(b.first_set(), Some(2));
    assert_eq!(b.last_set(), Some(200));
    assert_eq!(b.first_clear(), 0);
    assert_eq!(b.count_set_bits(), 6);

    // `next_set(i)` / `next_clear(i)` start looking at bit `i` itself.
    assert_eq!(b.next_set(5), Some(64));
    assert_eq!(b.next_set(201), None);
    assert_eq!(b.next_clear(2), 5);
    assert_eq!(b.next_clear(10_000), 10_000); // past the end every bit is clear

    // An all ones bitmap has no clear bit inside: the answer is its length.
    // Here the length depends on the word size, so fix it: `Bitmap64` aliases
    // `Bitmap<u64>`, two words are 128 bits on every target.
    let full = Bitmap64::from_words(vec![!0, !0]);
    assert_eq!(full.first_clear(), 128);
    assert_eq!(NativeBitmap::new().first_set(), None);
    assert_eq!(NativeBitmap::new().last_set(), None);

    // Iterator over the set bits, in increasing order.
    let ones: Vec<usize> = b.iter_ones().collect();
    assert_eq!(ones, [2, 3, 4, 64, 65, 200]);

    // `&BitSlice` is `IntoIterator` too (via `Deref`, a `&Bitmap` as well).
    let mut sum = 0;
    for bit in &b {
        sum += bit;
    }
    assert_eq!(sum, ones.iter().sum::<usize>());

    // The iterator knows how many bits remain, and `count` is a popcount.
    let mut it = b.iter_ones();
    assert_eq!(it.size_hint(), (6, Some(6)));
    it.next();
    assert_eq!(it.clone().count(), 5);

    // The same walk, restarting the search for each bit.
    let mut cur = b.first_set();
    let mut walked = Vec::new();
    while let Some(i) = cur {
        walked.push(i);
        cur = b.next_set(i + 1);
    }
    assert_eq!(walked, ones);

    println!("search: ok");
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
