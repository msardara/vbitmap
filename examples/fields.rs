// SPDX-License-Identifier: MIT OR Apache-2.0
//! Multi-bit fields, regions, bulk setting and random fill.
//!
//! Tip: unless you need to know the word size, use `NativeBitmap`
//! (`Bitmap<usize>`): `usize` is the machine's native word (64 bits on 64-bit
//! targets, 32 on 32-bit ones), so every step
//! handles a full register. Where an example asserts something that depends
//! on the word size (word counts, raw words) it uses `Bitmap64`, which aliases
//! `Bitmap<u64>`.
//!
//! Run with `cargo run --example fields`.

#[cfg(has_alloc)]
fn main() {
    use vbitmap::NativeBitmap;

    // `NativeBitmap` aliases `Bitmap<usize>`; fields up to 64 bits work for
    // every word size, so nothing here depends on it.
    let mut b = NativeBitmap::new();

    // A field of up to 64 bits can straddle two words, whatever the word type.
    b.set_field(60, 0xabcd, 16); // grows as needed
    assert_eq!(b.get_field(60, 16), 0xabcd);
    assert_eq!(b.get_field(60, 4), 0xd);
    assert_eq!(b.get_field(1000, 8), 0); // beyond the end reads as zero
    b.set_field(0, 0, 0); // zero width: nothing happens

    // `get_field_strict` / `set_field_strict`: the field must be in
    // range (and, for the read, inside one word); they never allocate.
    assert_eq!(b.get_field_strict(60, 4), 0xd);
    b.set_field_strict(4, 0b1010, 4);
    assert_eq!(b.get_field(4, 4), 0b1010);

    // Zeros written past the end only clear what exists, they do not grow it.
    let before = b.len_words();
    b.set_field(10_000, 0, 8);
    assert_eq!(b.len_words(), before);

    // Regions: set or clear a run of bits.
    let mut r = NativeBitmap::new();
    r.set_region(10, true, 100);
    assert_eq!(r.count_set_bits(), 100);
    assert_eq!((r.first_set(), r.last_set()), (Some(10), Some(109)));
    r.set_region(20, false, 10);
    assert_eq!(r.count_set_bits(), 90);
    r.set_region(0, true, 0); // empty region: nothing happens
    r.set_region(1_000_000, false, 5); // clearing past the end: nothing happens
    assert!(r.len_words() < 10);

    // Set many bits from any iterator; the pointer and length stay in
    // registers, which is faster than calling `set` in a loop.
    let mut m = NativeBitmap::new();
    m.set_many([3usize, 500, 7, 7]);
    m.set_many((0..10).map(|i| i * 100));
    assert!(m.get(3) && m.get(500) && m.get(900));

    // Random fill of `n` bits; bits at or above `n` stay zero.
    let mut state = 0x9e37_79b9_7f4a_7c15u64; // xorshift64: bring your own RNG
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut rand = NativeBitmap::new();
    rand.random(100, &mut rng);
    assert!(rand.last_set().is_none_or(|p| p < 100));
    rand.random(0, &mut rng); // zero bits: empty bitmap
    assert_eq!(rand.len_words(), 0);

    println!("fields: ok");
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
