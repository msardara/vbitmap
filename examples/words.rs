// SPDX-License-Identifier: MIT OR Apache-2.0
//! Word types, the `Word` trait and borrowed views.
//!
//! Tip: unless you need a fixed layout, use the native word, i.e.
//! `NativeBitmap` (`Bitmap<usize>`): 64 bits on 64-bit targets and 32 on
//! 32-bit ones. The loop below shows the other
//! word types behave identically; they are not faster.
//!
//! Run with `cargo run --example words`.

#[cfg(has_alloc)]
fn main() {
    use vbitmap::{BitSlice, Bitmap, Word};

    // The same code works for every word type.
    fn fill<W: Word>() -> (usize, Vec<usize>) {
        let mut b = Bitmap::<W>::new();
        b.set_region(5, true, 40);
        b.set(200);
        // fields up to 64 bits work for every word size
        b.set_field(100, 0xdead_beef, 32);
        assert_eq!(b.get_field(100, 32), 0xdead_beef);
        (b.len_words(), b.iter_ones().take(3).collect())
    }
    for (name, (words, head)) in [
        ("u8", fill::<u8>()),
        ("u16", fill::<u16>()),
        ("u32", fill::<u32>()),
        ("u64", fill::<u64>()),
        ("u128", fill::<u128>()),
        ("usize", fill::<usize>()),
    ] {
        println!("{name:>5}: {words:>2} words, first bits {head:?}");
        assert_eq!(head, [5, 6, 7]);
    }

    // `Word` exposes what generic code needs.
    assert_eq!(<u16 as Word>::BITS, 16);
    assert_eq!(<u8 as Word>::low_mask(3), 0b111);
    assert_eq!(<u32 as Word>::bit(31), 1 << 31);
    assert_eq!(<u64 as Word>::clear_lowest_set(0b1100), 0b1000);
    assert_eq!(<u128 as Word>::from_u64(7).to_u64(), 7);
    assert_eq!(
        (<u8 as Word>::ZERO, <u8 as Word>::ONE, <u8 as Word>::ONES),
        (0, 1, 255)
    );

    // A `BitSlice` views any `[W]` without copying: a `Vec`, an array, a
    // slice of somebody else's memory.
    let mut words = [0u16; 4];
    {
        let view = BitSlice::new_mut(&mut words);
        view.set(3);
        view.set_field_strict(20, 0b111, 3);
        view.zero();
        view.set(40);
        assert_eq!(view.len_words(), 4);
        assert_eq!((view.len_bits(), view.bytes()), (64, 8));
        view.as_words_mut()[0] = 1;
    }
    let view: &BitSlice<u16> = words[..].as_ref(); // `AsRef<BitSlice<W>> for [W]`
    assert_eq!(view.iter_ones().collect::<Vec<_>>(), [0, 40]);
    assert_eq!(view.as_words()[0], 1);

    // A view and an owned bitmap compare equal when their set bits match.
    let owned = Bitmap::<u16>::from_words(words.to_vec());
    assert_eq!(&*owned, view);

    println!("words: ok");
}

#[cfg(not(has_alloc))]
fn main() {
    println!("this example needs the `alloc` feature");
}
