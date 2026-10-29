// SPDX-License-Identifier: MIT OR Apache-2.0
//! Formatting and parsing.
//!
//! Tip: unless you need to know the word size, use `NativeBitmap`
//! (`Bitmap<usize>`): `usize` is the machine's native word (64 bits on 64-bit
//! targets, 32 on 32-bit ones), so every step
//! handles a full register. Where an example asserts something that depends
//! on the word size (word counts, raw words) it uses `Bitmap64`, which aliases
//! `Bitmap<u64>`.
//!
//! Run with `cargo run --example text`.

#[cfg(has_alloc)]
fn main() {
    use vbitmap::{BitSlice, NativeBitmap, ParseError};

    // `NativeBitmap` aliases `Bitmap<usize>`; text output does not depend on
    // the word size.

    // Range lists: "0-3,5-7,11"; spaces are fine and one trailing comma is ok.
    let b = NativeBitmap::from_list("0-3, 5-7,11,").unwrap();
    assert_eq!(b.count_set_bits(), 8);
    // `FromStr` is the same parser.
    assert_eq!("2-4".parse::<NativeBitmap>().unwrap().format_list(), "2-4");

    // Hex masks: most significant digit first, optional `0x`.
    let h = NativeBitmap::from_hex_mask("0x8ef").unwrap();
    assert_eq!(h, b);

    // Errors carry no detail; `ParseError` is `Display` and `core::error::Error`.
    for bad in ["", "3-1", "1,,2", "x", "99999999999"] {
        assert_eq!(NativeBitmap::from_list(bad), Err(ParseError));
    }
    assert!(NativeBitmap::from_hex_mask("12g").is_err());
    assert_eq!(ParseError.to_string(), "invalid bitmap");

    assert_eq!(b.format_list(), "0-3, 5-7, 11");
    assert_eq!(b.format_hex(), "8ef");
    assert_eq!(NativeBitmap::new().format_hex(), "0");
    assert_eq!(b.to_string(), "0-3, 5-7, 11"); // `Display`
    assert_eq!(format!("{b:?}"), "Bitmap[0-3, 5-7, 11]"); // `Debug`

    let mut s = String::new();
    b.write_list(&mut s).unwrap();
    s.push('|');
    b.write_hex(&mut s).unwrap();
    assert_eq!(s, "0-3, 5-7, 11|8ef");

    // A `BitSlice` over plain words formats the same way.
    let words = [0b1011u8];
    let slice = BitSlice::new(&words);
    assert_eq!(format!("{slice} / {slice:?}"), "0-1, 3 / BitSlice[0-1, 3]");

    println!("text: ok ({})", b);
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
