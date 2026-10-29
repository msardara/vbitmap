// SPDX-License-Identifier: MIT OR Apache-2.0
//! Randomized differential tests against a `Vec<bool>` model, run for every
//! word type.
#![cfg(has_alloc)]

use vbitmap::{BitSlice, Bitmap, Word};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn check_trimmed<W: Word>(b: &Bitmap<W>) {
    assert!(
        b.as_words().last().is_none_or(|&w| w != W::ZERO),
        "not trimmed: {b:?}"
    );
}

fn random_bitmap<W: Word>(r: &mut Rng, n_bits: usize, density: usize) -> (Bitmap<W>, Vec<bool>) {
    let mut b = Bitmap::new();
    let mut m = vec![false; n_bits];
    for (i, slot) in m.iter_mut().enumerate() {
        if r.below(density) == 0 {
            b.set(i);
            *slot = true;
        }
    }
    (b, m)
}

fn set_get_and_trim<W: Word>() {
    let mut r = Rng(0x1234_5678_9abc_def1);
    let mut b = Bitmap::<W>::new();
    let mut m = vec![false; 700];
    for _ in 0..20_000 {
        let i = r.below(700);
        let v = r.below(2) == 1;
        if v {
            b.set(i);
        } else {
            b.unset(i);
        }
        m[i] = v;
        let mut t = b.clone();
        t.trim();
        check_trimmed(&t);
    }
    for (i, &bit) in m.iter().enumerate() {
        assert_eq!(b.get(i), bit);
    }
    assert!(!b.get(100_000));
    assert_eq!(b.count_set_bits(), m.iter().filter(|&&x| x).count());
}

fn searches<W: Word>() {
    let mut r = Rng(42);
    for &(n, d) in &[
        (1, 1),
        (64, 2),
        (65, 3),
        (500, 7),
        (4096, 200),
        (4096, 1000),
    ] {
        let (b, m) = random_bitmap::<W>(&mut r, n, d);
        assert_eq!(b.first_set(), m.iter().position(|&x| x));
        assert_eq!(b.last_set(), m.iter().rposition(|&x| x));
        assert_eq!(b.is_zero(), !m.iter().any(|&x| x));
        let mut padded = m.clone();
        padded.resize(b.len_bits(), false);
        assert_eq!(
            b.first_clear(),
            padded.iter().position(|&x| !x).unwrap_or(padded.len())
        );
        for start in [0, 1, 63, 64, 65, n / 2, n.saturating_sub(1), n, n + 70] {
            let ns = (start..m.len()).find(|&i| m[i]);
            assert_eq!(b.next_set(start), ns, "next_set({start})");
            let nc = if start >= b.len_bits() {
                start
            } else {
                (start..)
                    .find(|&i| !padded.get(i).copied().unwrap_or(false))
                    .unwrap()
            };
            assert_eq!(b.next_clear(start), nc, "next_clear({start})");
        }
        let ones: Vec<usize> = b.iter_ones().collect();
        let exp: Vec<usize> = (0..m.len()).filter(|&i| m[i]).collect();
        assert_eq!(ones, exp);
    }
}

fn logic_ops<W: Word>() {
    let mut r = Rng(7);
    for _ in 0..300 {
        let (na, nb) = (1 + r.below(600), 1 + r.below(600));
        let da = 1 + r.below(30);
        let (a, ma) = random_bitmap::<W>(&mut r, na, da);
        let db = 1 + r.below(30);
        let (b, mb) = random_bitmap::<W>(&mut r, nb, db);
        let n = na.max(nb) + 64;
        let get = |m: &Vec<bool>, i: usize| m.get(i).copied().unwrap_or(false);
        type Op = fn(bool, bool) -> bool;
        let cases: [(&str, Bitmap<W>, Op, bool); 4] = [
            ("and", a.dup_and(&b), |x, y| x & y, true),
            ("andnot", a.dup_andnot(&b), |x, y| x & !y, true),
            ("or", a.dup_or(&b), |x, y| x | y, false),
            ("xor", a.dup_xor(&b), |x, y| x ^ y, true),
        ];
        for (name, got, f, trims) in cases {
            for i in 0..n {
                assert_eq!(got.get(i), f(get(&ma, i), get(&mb, i)), "{name} bit {i}");
            }
            if trims {
                check_trimmed(&got);
            }
        }
        // operator forms, and operands that are plain word slices
        let mut c = a.clone();
        c |= BitSlice::new(b.as_words());
        assert_eq!(c, a.dup_or(&b));
    }
}

fn immediates<W: Word>() {
    let far = W::BITS * 2 + 2;
    let mut b = Bitmap::<W>::new();
    b.ori(far);
    assert_eq!(b.len_words(), 3);
    b.xori(far);
    assert_eq!(b.len_words(), 0);
    b.ori(1);
    b.ori(W::BITS + 1);
    b.andnoti(W::BITS + 1);
    assert_eq!(b.len_words(), 1);
    b.andnoti_notrim(1);
    assert_eq!(b.len_words(), 1);
    assert!(b.is_zero());
}

fn field_and_region<W: Word>() {
    let maxk = W::BITS.min(64);
    let mut r = Rng(99);
    for _ in 0..2000 {
        let n = 1 + r.below(1000);
        let (mut b, mut m) = random_bitmap::<W>(&mut r, n, 3);
        let i = r.below(n);
        let k = 1 + r.below(maxk);
        let v = r.next();
        b.set_field(i, v, k);
        if m.len() < i + k {
            m.resize(i + k, false);
        }
        for j in 0..k {
            m[i + j] = (v >> j) & 1 != 0;
        }
        let mut exp = 0u64;
        for j in 0..k {
            exp |= (m[i + j] as u64) << j;
        }
        assert_eq!(b.get_field(i, k), exp);
        for (idx, bit) in m.iter().enumerate() {
            assert_eq!(b.get(idx), *bit);
        }

        let i = r.below(n);
        let k = 1 + r.below(300);
        let val = r.below(2) == 1;
        b.set_region(i, val, k);
        if m.len() < i + k {
            m.resize(i + k, false);
        }
        m[i..i + k].fill(val);
        for (idx, bit) in m.iter().enumerate() {
            assert_eq!(b.get(idx), *bit, "region i={i} k={k} idx={idx}");
        }
    }
    let b = Bitmap::<W>::alloc(64);
    assert_eq!(b.get_field(60, 4), 0);
    assert_eq!(b.get_field(1000, 4), 0);
}

fn single_bit_no_check<W: Word>() {
    let mut b = Bitmap::<W>::alloc(200);
    let s: &mut BitSlice<W> = &mut b;
    s.set(150);
    s.set(150); // idempotent
    s.set_many_strict([10, 199, 150]);
    assert_eq!(s.count_set_bits(), 3);
    s.unset(10);
    s.unset(199);
    assert!(s.get_strict(150));
    s.unset(150);
    assert!(s.is_zero());
    assert_eq!(b.len_words(), 200usize.div_ceil(W::BITS));
}

fn text<W: Word>() {
    let b = Bitmap::<W>::from_list("0-3,5-7,11").unwrap();
    assert_eq!(b.format_list(), "0-3, 5-7, 11");
    assert_eq!(b.format_hex(), "8ef");
    assert_eq!(
        Bitmap::<W>::from_hex_mask("0x8ef").unwrap().format_list(),
        "0-3, 5-7, 11"
    );
    assert_eq!(Bitmap::<W>::new().format_hex(), "0");
    assert_eq!(Bitmap::<W>::new().format_list(), "");
    assert!(Bitmap::<W>::from_list("3-1").is_err());
    assert!(Bitmap::<W>::from_list("").is_err());
    assert!(Bitmap::<W>::from_list("a").is_err());
    assert!(Bitmap::<W>::from_hex_mask("xyz").is_err());
    let big = Bitmap::<W>::from_hex_mask("1ffffffffffffffff0").unwrap();
    assert_eq!(big.first_set(), Some(4));
    assert_eq!(big.last_set(), Some(68));
    assert_eq!(big.format_hex(), "1ffffffffffffffff0");
    assert_eq!(Bitmap::<W>::from_list("1,2,").unwrap().format_list(), "1-2");
    assert!(Bitmap::<W>::from_list("1,,2").is_err());
    assert!(Bitmap::<W>::from_list(",1").is_err());
}

fn random_masks_tail<W: Word>() {
    let mut r = Rng(5);
    let mut b = Bitmap::<W>::new();
    b.random(70, || r.next());
    assert_eq!(b.len_words(), 70usize.div_ceil(W::BITS));
    assert!(b.last_set().is_none_or(|p| p < 70));
}

fn borrowed_views<W: Word>() {
    let words = [W::ONE, W::ZERO, W::ONE << 3];
    let s = BitSlice::new(&words);
    assert_eq!(s.iter_ones().collect::<Vec<_>>(), [0, 2 * W::BITS + 3]);
    let mut arr = [W::ZERO; 4];
    let s = BitSlice::new_mut(&mut arr);
    s.set(W::BITS + 1);
    assert_eq!(s.first_set(), Some(W::BITS + 1));
}

fn eq_ignores_trailing_zeros<W: Word>() {
    use std::hash::{BuildHasher, RandomState};
    let h = RandomState::new();
    assert_eq!(Bitmap::<W>::from_hex_mask("0").unwrap(), Bitmap::new());
    assert_eq!(
        Bitmap::<W>::from_hex_mask("00ff").unwrap(),
        Bitmap::<W>::from_hex_mask("ff").unwrap()
    );
    let mut a = Bitmap::<W>::new();
    a.set(3);
    let mut b = Bitmap::<W>::alloc(1000);
    b.set(3);
    assert_eq!(a, b);
    assert_eq!(h.hash_one(&a), h.hash_one(&b));
    b.set(900);
    assert_ne!(a, b);

    // clearing past the end must not allocate
    let mut c = Bitmap::<W>::new();
    c.set_region(200, false, 5);
    c.set_field(500, 0, 8);
    assert_eq!(c.len_words(), 0);
    // ... but still clears what exists
    let mut d = Bitmap::<W>::alloc(64);
    d.set_region(0, true, 64);
    d.set_region(60, false, 100);
    assert_eq!(d.count_set_bits(), 60);

    let mut r = Rng(1);
    for _ in 0..200 {
        let mut x = Bitmap::<W>::new();
        x.random(1, || r.next());
        check_trimmed(&x);
    }
}

fn wide_get_set_field<W: Word>() {
    // up to 64 bits must work for every word size
    let mut r = Rng(77);
    for _ in 0..500 {
        let mut b = Bitmap::<W>::new();
        let i = r.below(300);
        let v = r.next();
        b.set_field(i, v, 64);
        assert_eq!(b.get_field(i, 64), v);
        for j in 0..64 {
            assert_eq!(b.get(i + j), (v >> j) & 1 != 0);
        }
        assert_eq!(b.get_field(i + 3, 40), (v >> 3) & ((1 << 40) - 1));
    }
    let b = Bitmap::<W>::from_words(vec![W::ONES; 2 + 128 / W::BITS]);
    assert_eq!(b.get_field(5, 64), !0);
}

fn dense_scans<W: Word>() {
    // exercise the 8-word chunked scans around chunk edges
    for n_words in [7usize, 8, 9, 15, 16, 17, 40] {
        let bits = n_words * W::BITS;
        for pos in [0, 1, bits / 2, bits - 1] {
            let mut b = Bitmap::<W>::alloc(bits);
            b.set(pos);
            assert_eq!(b.first_set(), Some(pos));
            assert_eq!(b.last_set(), Some(pos));
            assert_eq!(b.next_set(0), Some(pos));
            let mut c = b.clone();
            c.trim();
            assert_eq!(c.len_words(), pos / W::BITS + 1);

            let mut f = Bitmap::<W>::from_words(vec![W::ONES; n_words]);
            f.unset(pos);
            assert_eq!(f.first_clear(), pos);
            assert_eq!(f.next_clear(0), pos);
            assert_eq!(
                f.next_clear(pos + 1),
                if pos + 1 < bits { bits } else { pos + 1 }
            );
        }
        let full = Bitmap::<W>::from_words(vec![W::ONES; n_words]);
        assert_eq!(full.first_clear(), bits);
        assert_eq!(full.iter_ones().count(), bits);
        assert_eq!(full.iter_ones().size_hint(), (bits, Some(bits)));
    }
}

fn set_many_matches_set<W: Word>() {
    let mut r = Rng(31);
    for _ in 0..200 {
        let idx: Vec<usize> = (0..r.below(200)).map(|_| r.below(1500)).collect();
        let mut a = Bitmap::<W>::new();
        for &i in &idx {
            a.set(i);
        }
        let mut b = Bitmap::<W>::new();
        b.set_many(idx.iter().copied());
        assert_eq!(a, b);
        assert!(a.as_words() == b.as_words());
    }
}

fn fused_ops_match_dup<W: Word>() {
    let mut r = Rng(2024);
    let mut x = Bitmap::<W>::from_words(vec![W::ONES; 30]); // stale contents
    for _ in 0..300 {
        let (na, nb) = (1 + r.below(600), 1 + r.below(600));
        let da = 1 + r.below(30);
        let (a, _) = random_bitmap::<W>(&mut r, na, da);
        let db = 1 + r.below(30);
        let (b, _) = random_bitmap::<W>(&mut r, nb, db);
        x.and_from(&a, &b);
        assert_eq!(x, a.dup_and(&b));
        check_trimmed(&x);
        x.andnot_from(&a, &b);
        assert_eq!(x, a.dup_andnot(&b));
        check_trimmed(&x);
        x.or_from(&a, &b);
        assert_eq!(x, a.dup_or(&b));
        assert_eq!(x.as_words().len(), a.len_words().max(b.len_words()));
        x.xor_from(&a, &b);
        assert_eq!(x, a.dup_xor(&b));
        check_trimmed(&x);
        // and the other way round
        x.or_from(&b, &a);
        assert_eq!(x, a.dup_or(&b));
    }
}

fn set_unset_match_across_views<W: Word>() {
    let mut r = Rng(555);
    let mut a = Bitmap::<W>::new();
    let mut b = Bitmap::<W>::new();
    let mut c = Bitmap::<W>::alloc(900);
    for _ in 0..5000 {
        let i = r.below(900);
        if r.below(2) == 0 {
            a.set(i);
            b.set(i);
            c.set(i);
        } else {
            a.unset(i);
            b.unset(i);
            c.unset(i);
        }
        assert_eq!(a, b);
        assert_eq!(a, c);
    }
    a.unset(1_000_000); // past the end: nothing
    assert_eq!(a, b);
}

fn popcount_all_lengths<W: Word>() {
    // lengths around the 8-word threshold and the 64-byte kernel chunk
    let mut r = Rng(321);
    for n_words in 0..70usize {
        let words: Vec<W> = (0..n_words)
            .map(|_| {
                let mut v = W::ZERO;
                for k in 0..(128 / W::BITS).max(1) {
                    v = v | (W::from_u64(r.next()) << (k * W::BITS.min(64) % W::BITS));
                }
                v
            })
            .collect();
        let expect: usize = words.iter().map(|w| w.count_ones() as usize).sum();
        let b = Bitmap::<W>::from_slice(&words);
        assert_eq!(b.count_set_bits(), expect, "{n_words} words");
    }
    // all ones, long enough to cross the accumulator block size
    let big = Bitmap::<W>::from_words(vec![W::ONES; 2000 * 64 / (W::BITS / 8) + 100]);
    assert_eq!(big.count_set_bits(), big.len_bits());
}

fn iter_ones_all_densities<W: Word>() {
    // The iterator picks a decoder from the density of the first words: check
    // every density against a bit-by-bit model, fully and partly consumed.
    let mut r = Rng(77);
    for d in [1usize, 2, 3, 4, 16, 64] {
        for n_bits in [100usize, 3000, 20_000] {
            let (b, m) = random_bitmap::<W>(&mut r, n_bits, d);
            let expect: Vec<usize> = (0..m.len()).filter(|&i| m[i]).collect();
            assert_eq!(
                b.iter_ones().collect::<Vec<_>>(),
                expect,
                "d={d} n={n_bits}"
            );
            assert_eq!(b.iter_ones().sum::<usize>(), expect.iter().sum::<usize>());
            let mut seen = 0usize;
            b.iter_ones().for_each(|i| seen += i);
            assert_eq!(seen, expect.iter().sum::<usize>());
            // start in the middle of a word and fold the rest
            let mut it = b.iter_ones();
            let skip = expect.len() / 3;
            for _ in 0..skip {
                it.next();
            }
            let rest: Vec<usize> = it.collect();
            assert_eq!(rest, expect[skip..]);
        }
    }
    // a bitmap that is all ones: densest case, partial first word
    let full = Bitmap::<W>::from_words(vec![W::ONES; 4000 / W::BITS + 70]);
    let mut it = full.iter_ones();
    it.next();
    it.next();
    assert_eq!(it.count(), full.len_bits() - 2);
    let mut it = full.iter_ones();
    it.next();
    assert_eq!(it.sum::<usize>(), (1..full.len_bits()).sum::<usize>());
}

fn try_get_matches_get<W: Word>() {
    let mut b = Bitmap::<W>::alloc(130);
    b.set(3);
    b.set(129);
    let len = b.len_bits();
    assert_eq!(b.try_get(3), Some(true));
    assert_eq!(b.try_get(4), Some(false));
    assert_eq!(b.try_get(129), Some(true));
    for i in 0..len + 200 {
        assert_eq!(b.try_get(i).unwrap_or(false), b.get(i), "bit {i}");
        assert_eq!(b.try_get(i).is_none(), i >= len, "bit {i}");
    }
    assert_eq!(b.try_get(usize::MAX), None);
}

macro_rules! for_words {
    ($($f:ident),*) => {$(
        mod $f {
            #[test] fn u8() { super::$f::<u8>() }
            #[test] fn u16() { super::$f::<u16>() }
            #[test] fn u32() { super::$f::<u32>() }
            #[test] fn u64() { super::$f::<u64>() }
            #[test] fn u128() { super::$f::<u128>() }
            #[test] fn usize() { super::$f::<usize>() }
        }
    )*};
}

for_words!(
    set_get_and_trim,
    searches,
    logic_ops,
    immediates,
    field_and_region,
    single_bit_no_check,
    text,
    random_masks_tail,
    borrowed_views,
    eq_ignores_trailing_zeros,
    wide_get_set_field,
    dense_scans,
    set_many_matches_set,
    fused_ops_match_dup,
    set_unset_match_across_views,
    try_get_matches_get,
    popcount_all_lengths,
    iter_ones_all_densities
);
