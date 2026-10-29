// SPDX-License-Identifier: MIT OR Apache-2.0
//! Exercises the whole public surface, including the rarely used corners.
#![cfg(has_alloc)]

use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};

use vbitmap::{BitSlice, Bitmap, Bitmap64, NativeBitSlice, NativeBitmap, ParseError, Word};

fn bm(bits: &[usize]) -> Bitmap<u64> {
    bits.iter().copied().collect()
}

#[test]
fn construction_and_storage() {
    let b = Bitmap::<u64>::with_capacity_bits(200);
    assert!(b.as_words().is_empty());
    assert!(!b.will_expand(100));
    assert!(b.will_expand(100_000));

    let mut b = Bitmap::<u64>::alloc(130);
    assert_eq!(b.len_words(), 3);
    assert_eq!(b.len_bits(), 192);
    assert_eq!(b.bytes(), 24);
    b.as_words_mut()[0] = 5;
    assert_eq!(b.count_set_bits(), 2);
    b.zero();
    assert!(b.is_zero());
    assert_eq!(b.len_words(), 3);
    b.clear();
    assert_eq!(b.len_words(), 0);

    b.validate(0);
    assert_eq!(b.len_words(), 0);
    b.validate(70);
    assert_eq!(b.len_words(), 2);
    b.validate(10);
    assert_eq!(b.len_words(), 2);

    let v = Bitmap::<u64>::from(vec![1, 2, 3]);
    assert_eq!(v.into_words(), vec![1, 2, 3]);
    assert_eq!(Bitmap64::from_words(vec![7]).as_words(), &[7]);

    let mut c = Bitmap::<u64>::new();
    c.copy_from(&bm(&[1, 100]));
    assert_eq!(c, bm(&[1, 100]));
    c.copy_from(&bm(&[3]));
    assert_eq!(c, bm(&[3]));

    let d: NativeBitmap = NativeBitmap::default();
    assert!(d.is_zero());
    let s: &NativeBitSlice = BitSlice::new(&[]);
    assert!(s.is_zero());
}

#[test]
fn single_bits() {
    let mut b = Bitmap::<u64>::new();
    b.unset(5); // beyond the end: no allocation
    assert_eq!(b.len_words(), 0);
    b.set(70);
    assert_eq!(b.len_words(), 2);
    b.unset(70); // `set` never shrinks...
    assert_eq!(b.len_words(), 2);
    assert!(b.is_zero());
    b.trim(); // ...`trim` drops the trailing zero words
    assert_eq!(b.len_words(), 0);
    b.set(1);
    b.set(70);
    b.unset(1);
    assert_eq!(b.len_words(), 2);
    b.unset(70);
    b.trim();
    assert_eq!(b.len_words(), 0);
    b.set(70);

    assert!(!b.get(1000));
    assert!(b.get_strict(70));
}

#[test]
fn field_edges() {
    let mut b = Bitmap::<u64>::new();
    b.set_field(10, 0xff, 0); // zero width is a no-op
    assert_eq!(b.len_words(), 0);
    b.set_field(500, 0, 8); // zeros past the end do not allocate
    assert_eq!(b.len_words(), 0);

    b.set_field(60, 0xff, 8);
    assert_eq!(b.get_field(60, 8), 0xff);
    assert_eq!(b.get_field_strict(60, 4), 0xf);
    // zeros straddling the end only clear what exists
    b.set_field(62, 0, 10);
    assert_eq!(b.get_field(60, 8), 0b11);
    assert_eq!(b.len_words(), 2);
    b.set_field(0, !0, 64);
    assert_eq!(b.get_field(0, 64), !0);
    // zeros straddling the end of a one word bitmap clear only the 4 bits there
    let mut one = Bitmap::<u64>::from_words(vec![!0]);
    one.set_field(60, 0, 8);
    assert_eq!(one.as_words(), &[!0u64 >> 4]);
    one.set_field_strict(0, 0xff, 0); // zero width is a no-op
    assert_eq!(one.as_words(), &[!0u64 >> 4]);

    b.set_region(0, true, 0); // no-op
    b.set_region(1000, false, 5); // clearing past the end does not allocate
    assert_eq!(b.len_words(), 2);
    b.set_region(3, false, 1000); // clamped to the end
    assert_eq!(b.count_set_bits(), 3);
}

/// A range of 512 bytes started at every word of two pages crosses a page
/// boundary for some starts, wherever the allocation lands.
#[test]
fn set_region_across_pages() {
    let len = 4094;
    let mut b = Bitmap::<u64>::alloc(64 * 1024 + 4096);
    for k in 0..1024 {
        let i = k * 64 + 1;
        for v in [true, false] {
            b.set_region(i, v, len);
            assert_eq!(b.count_set_bits(), if v { len } else { 0 });
            assert_eq!(b.first_set(), if v { Some(i) } else { None });
            assert_eq!(b.last_set(), if v { Some(i + len - 1) } else { None });
        }
    }
}

#[test]
fn bulk_logic() {
    let a = bm(&[1, 2, 70, 200]);
    let b = bm(&[2, 70, 71]);

    let mut x = a.clone();
    x.and(&b);
    assert_eq!(x, bm(&[2, 70]));
    let mut x = a.clone();
    x &= &b;
    assert_eq!(x, bm(&[2, 70]));

    let mut x = a.clone();
    x.andnot(&b);
    assert_eq!(x, bm(&[1, 200]));

    let mut x = a.clone();
    x.or(&b);
    assert_eq!(x, bm(&[1, 2, 70, 71, 200]));
    let mut x = b.clone();
    x |= &a; // grows
    assert_eq!(x, bm(&[1, 2, 70, 71, 200]));

    let mut x = a.clone();
    x.xor(&b);
    assert_eq!(x, bm(&[1, 71, 200]));
    let mut x = b.clone();
    x ^= &a; // grows
    assert_eq!(x, bm(&[1, 71, 200]));

    assert_eq!(a.dup_and(&b), bm(&[2, 70]));
    assert_eq!(a.dup_andnot(&b), bm(&[1, 200]));
    assert_eq!(a.dup_or(&b), bm(&[1, 2, 70, 71, 200]));
    assert_eq!(b.dup_or(&a), bm(&[1, 2, 70, 71, 200]));
    assert_eq!(a.dup_xor(&b), bm(&[1, 71, 200]));
    assert_eq!(b.dup_xor(&a), bm(&[1, 71, 200]));
}

#[test]
fn immediates() {
    let mut b = bm(&[3]);
    b.andi(1000); // past the end: nothing to do
    assert_eq!(b, bm(&[3]));
    b.andnoti(1000);
    assert_eq!(b, bm(&[3]));
    b.andi(3); // word stays non-zero
    assert_eq!(b.len_words(), 1);
    b.andnoti(3); // word becomes zero: trimmed
    assert_eq!(b.len_words(), 0);

    // the *_notrim variants leave trailing zero words in place
    let mut b = Bitmap::<u64>::new();
    b.ori(70);
    b.andi_notrim(64); // word 1 &= 1 << 0, which clears bit 70
    assert_eq!(b.len_words(), 2);
    assert!(b.is_zero());
    b.ori(65);
    b.andnoti_notrim(65);
    assert_eq!(b.len_words(), 2);
    b.xori_notrim(66);
    b.xori_notrim(66);
    assert_eq!(b.len_words(), 2);

    // the trimming variants cut back to the last non-zero word
    let mut b = bm(&[1, 200]);
    b.xori(200); // last word becomes zero
    assert_eq!(b.len_words(), 1);
    b.xori(1);
    assert_eq!(b.len_words(), 0);
    b.xori(1); // sets it again, word stays non-zero
    assert_eq!(b.len_words(), 1);
    b.andi(1); // keeps bit 1
    assert_eq!(b.len_words(), 1);
    b.andi(2); // word &= 1 << 2 clears it: trimmed
    assert_eq!(b.len_words(), 0);
}

#[test]
fn iteration_and_conversion() {
    let b = bm(&[0, 5, 64, 130]);
    assert_eq!(b.iter_ones().collect::<Vec<_>>(), [0, 5, 64, 130]);
    assert_eq!((&b).into_iter().count(), 4);
    assert_eq!((&*b).into_iter().count(), 4);
    let mut it = b.iter_ones();
    assert_eq!(it.size_hint(), (4, Some(4)));
    it.next();
    assert_eq!(it.size_hint(), (3, Some(3)));
    for _ in it.by_ref() {}
    assert_eq!(it.size_hint(), (0, Some(0)));
    assert_eq!(it.next(), None);
    assert_eq!(it.next(), None);

    let words = [1u64, 0, 1];
    let s: &BitSlice<u64> = words[..].as_ref();
    assert_eq!(s.last_set(), Some(128));
    assert!(Bitmap::<u64>::new().last_set().is_none());
    assert!(Bitmap::<u64>::new().first_set().is_none());
}

#[test]
fn random_fill() {
    let mut r = 0x9e37_79b9_7f4a_7c15u64;
    let mut rng = || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        r
    };
    let mut b = Bitmap::<u64>::new();
    b.random(0, &mut rng);
    assert_eq!(b.len_words(), 0);
    b.random(64, &mut rng);
    assert!(b.len_words() <= 1);
    b.random(100, &mut rng);
    assert!(b.last_set().is_none_or(|p| p < 100));
    let mut w = Bitmap::<u128>::new();
    w.random(300, &mut rng);
    assert!(w.last_set().is_none_or(|p| p < 300));
    let mut n = Bitmap::<u8>::new();
    n.random(50, &mut rng);
    assert!(n.last_set().is_none_or(|p| p < 50));
}

#[test]
fn text_round_trips() {
    let b = bm(&[0, 1, 2, 3, 5, 6, 7, 11]);
    assert_eq!(b.to_string(), "0-3, 5-7, 11");
    assert_eq!(format!("{b:?}"), "Bitmap[0-3, 5-7, 11]");
    assert_eq!(format!("{:?}", &*b), "BitSlice[0-3, 5-7, 11]");
    assert_eq!((*b).to_string(), "0-3, 5-7, 11");
    assert_eq!("0-3,5-7,11".parse::<Bitmap<u64>>().unwrap(), b);
    assert_eq!(b.format_hex(), "8ef");

    let mut s = String::new();
    b.write_hex(&mut s).unwrap();
    assert_eq!(s, "8ef");
    let mut s = String::new();
    Bitmap::<u64>::new().write_hex(&mut s).unwrap();
    assert_eq!(s, "0");
    // a multi word value keeps inner zero digits
    let big = Bitmap::<u64>::from_hex_mask("1_0000_0000_0000_0001".replace('_', "").as_str());
    assert_eq!(big.unwrap().format_hex(), "10000000000000001");
}

#[test]
fn parse_errors() {
    let e = Bitmap::<u64>::from_list("x").unwrap_err();
    assert_eq!(e, ParseError);
    assert_eq!(e.to_string(), "invalid bitmap");
    let dynerr: &dyn Error = &e;
    assert!(dynerr.source().is_none());

    for bad in [
        "",
        "   ",
        "3-1",
        "1-",
        "-1",
        "a-b",
        "1,,2",
        ",1",
        "99999999999",
    ] {
        assert!(Bitmap::<u64>::from_list(bad).is_err(), "{bad:?}");
    }
    for bad in ["", "0x", "xyz", "12g"] {
        assert!(Bitmap::<u64>::from_hex_mask(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        Bitmap::<u64>::from_hex_mask(" 0XFF ").unwrap(),
        bm(&[0, 1, 2, 3, 4, 5, 6, 7])
    );
    assert_eq!(
        Bitmap::<u64>::from_list(" 1 , 3-4 ").unwrap(),
        bm(&[1, 3, 4])
    );
}

#[test]
fn equality_and_hash() {
    let h = |b: &Bitmap<u64>| {
        let mut s = DefaultHasher::new();
        b.hash(&mut s);
        s.finish()
    };
    let a = bm(&[3]);
    let mut b = Bitmap::<u64>::alloc(512);
    b.set(3);
    assert_eq!(a, b);
    assert_eq!(h(&a), h(&b));
    assert_ne!(a, bm(&[4]));
    let hs = |s: &BitSlice<u64>| {
        let mut st = DefaultHasher::new();
        s.hash(&mut st);
        st.finish()
    };
    assert_eq!(hs(&a), hs(&b));
}

#[test]
fn word_trait_helpers() {
    assert_eq!(<u8 as Word>::low_mask(0), 0);
    assert_eq!(<u8 as Word>::low_mask(8), 0xff);
    assert_eq!(<u32 as Word>::low_mask(40), u32::MAX);
    assert_eq!(<u64 as Word>::bit(63), 1 << 63);
    assert_eq!(<u16 as Word>::clear_lowest_set(0b1100), 0b1000);
    assert_eq!(<u128 as Word>::from_u64(5).to_u64(), 5);
}

#[test]
#[should_panic(expected = "bit index overflow")]
fn set_region_overflow_panics() {
    Bitmap::<u64>::new().set_region(usize::MAX - 1, true, 4);
}

#[test]
#[should_panic(expected = "bit index overflow")]
fn set_field_overflow_panics() {
    Bitmap::<u64>::new().set_field(usize::MAX - 1, 1, 8);
}

#[test]
#[should_panic]
fn set_many_strict_out_of_range_panics() {
    let mut b = Bitmap::<u64>::alloc(64);
    b.set_many_strict([1, 64]);
}

#[test]
fn set_many_strict_keeps_bits_before_the_panic() {
    let mut b = Bitmap::<u64>::alloc(64);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.set_many_strict([1, 64])));
    assert!(r.is_err());
    assert!(b.get(1));
    assert_eq!(b.len_words(), 1);
}

#[test]
#[should_panic]
fn get_field_too_wide_panics() {
    let _ = Bitmap::<u64>::new().get_field(0, 65);
}

#[test]
#[should_panic]
fn set_field_too_wide_panics() {
    Bitmap::<u64>::new().set_field(0, 0, 65);
}

#[test]
fn storage_is_cache_line_aligned() {
    fn aligned<W: Word>() {
        for n in [1usize, 100, 1000, 100_000] {
            let b = Bitmap::<W>::alloc(n);
            assert_eq!(b.as_words().as_ptr() as usize % 64, 0, "{n} bits");
        }
        let mut b = Bitmap::<W>::new();
        b.set(5000); // grown from empty
        assert_eq!(b.as_words().as_ptr() as usize % 64, 0);
        let c = b.clone();
        assert_eq!(c.as_words().as_ptr() as usize % 64, 0);
        assert_ne!(c.as_words().as_ptr(), b.as_words().as_ptr());
    }
    aligned::<u8>();
    aligned::<u16>();
    aligned::<u32>();
    aligned::<u64>();
    aligned::<u128>();
    aligned::<usize>();
}

#[test]
fn regrowing_after_shrinking_reads_zero() {
    // Words past the length keep their old contents in memory; growing the
    // bitmap again must not expose them.
    let mut b = Bitmap::<u64>::from_words(vec![!0; 20]);
    b.and(&bm(&[1])); // shrinks to one word
    assert_eq!(b.len_words(), 1);
    b.validate(20 * 64);
    assert_eq!(b.len_words(), 20);
    assert_eq!(b.count_set_bits(), 1);

    let mut b = Bitmap::<u64>::from_words(vec![!0; 20]);
    b.clear();
    b.set(1000);
    assert_eq!(b.count_set_bits(), 1);

    let mut b = Bitmap::<u64>::from_words(vec![!0; 20]);
    b.copy_from(&bm(&[3]));
    b.or(&Bitmap::<u64>::from_words(vec![0; 10]));
    assert_eq!(b.count_set_bits(), 1);
    assert_eq!(b.len_words(), 10);

    // `from_slice` and `into_words` round trip
    let words = [1u64, 2, 3];
    assert_eq!(Bitmap::<u64>::from_slice(&words).into_words(), words);
}

#[test]
fn iteration_fold_matches_next() {
    // `sum`/`for_each` go through the internal `fold`; compare with `next`
    for n in [0usize, 1, 63, 64, 65, 500] {
        let b: Bitmap<u64> = (0..n).filter(|i| i % 3 != 1).collect();
        // deliberately drives `next` by hand (not a `for` loop)
        #[allow(clippy::while_let_on_iterator)]
        let by_next: usize = {
            let mut it = b.iter_ones();
            let mut s = 0;
            while let Some(i) = it.next() {
                s += i;
            }
            s
        };
        assert_eq!(b.iter_ones().sum::<usize>(), by_next);
        let mut seen = Vec::new();
        b.iter_ones().for_each(|i| seen.push(i));
        assert_eq!(seen, b.iter_ones().collect::<Vec<_>>());
        // folding a partly consumed iterator continues where it stopped
        let mut it = b.iter_ones();
        let first = it.next();
        let rest: Vec<usize> = it.collect();
        assert_eq!(first.into_iter().chain(rest).collect::<Vec<_>>(), seen);
    }
}
