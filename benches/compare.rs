// SPDX-License-Identifier: MIT OR Apache-2.0
//! `vbitmap` against the most downloaded bitmap and bitset crates, on
//! identical data. One Criterion group per operation, one function per crate,
//! one parameter per bitmap size. `just compare` runs it and prints the tables.
//!
//! Heap layout moves some rows of a few nanoseconds by tens of percent (a small
//! bitmap that straddles a page boundary, say). `LAYOUT_SHIFT=<bytes>` offsets
//! every later allocation, so runs with different shifts sample different
//! layouts and the minimum over runs is not one layout's luck.
#![cfg(has_alloc)]

use std::hint::black_box;
use std::time::Duration;

use bit_set::BitSet;
use bit_vec::BitVec as BitVecCrate;
use bitvec::prelude::*;
use bv::{Bits, BitsMut};
use criterion::{BenchmarkId, Criterion};
use fixedbitset::FixedBitSet;
use vbitmap::{BitSlice, NativeBitmap};

const SIZES: [usize; 3] = [256, 4096, 262_144];
const BATCH: usize = 1024;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Each bit set with probability 1/`density`; the top bit is always set.
fn make_bits(n: usize, density: u64, seed: u64) -> Vec<bool> {
    let mut r = Rng(seed);
    let mut v: Vec<bool> = (0..n).map(|_| r.next() % density == 0).collect();
    v[n - 1] = true;
    v
}

fn indices(limit: usize, seed: u64) -> Vec<usize> {
    let mut r = Rng(seed);
    (0..BATCH)
        .map(|_| (r.next() % limit as u64) as usize)
        .collect()
}

fn vb(bits: &[bool]) -> NativeBitmap {
    let mut b = NativeBitmap::alloc(bits.len());
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set(i);
    }
    b
}
fn fb(bits: &[bool]) -> FixedBitSet {
    let mut b = FixedBitSet::with_capacity(bits.len());
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.insert(i);
    }
    b
}
fn bvec(bits: &[bool]) -> BitVecCrate<usize> {
    let mut b = BitVecCrate::<usize>::from_elem_general(bits.len(), false);
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set(i, true);
    }
    b
}
fn bset(bits: &[bool]) -> BitSet<usize> {
    BitSet::from_bit_vec_general(bvec(bits))
}
fn bvc(bits: &[bool]) -> BitVec<usize, Lsb0> {
    let mut b = BitVec::<usize, Lsb0>::repeat(false, bits.len());
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set(i, true);
    }
    b
}
fn bvv(bits: &[bool]) -> bv::BitVec<usize> {
    let mut b = bv::BitVec::<usize>::new_fill(false, bits.len() as u64);
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set_bit(i as u64, true);
    }
    b
}
/// `bitmaps` only exists up to 1024 bits, so it is measured at 256 only.
fn bmaps(bits: &[bool]) -> bitmaps::Bitmap<256> {
    let mut b = bitmaps::Bitmap::<256>::new();
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set(i, true);
    }
    b
}

/// `add!(group, "crate", n, expr)` times `expr` once per iteration.
macro_rules! add {
    ($g:expr, $name:expr, $n:expr, $body:expr) => {
        $g.bench_function(BenchmarkId::new($name, $n), |bn| bn.iter(|| $body))
    };
}

/// The same operation on all six single-bitmap representations.
macro_rules! unary {
    ($c:expr, $op:expr, $bits:expr, $n:expr, $a:ident, $b:ident, $d:ident, $e:ident, $f:ident,
     $va:expr, $vb:expr, $nc:literal, $vc:expr, $vd:expr, $ve:expr, $vg:expr) => {{
        let bits = $bits;
        let ($a, $b, $d, $e, $f) = (vb(&bits), fb(&bits), bvec(&bits), bset(&bits), bvc(&bits));
        let mut grp = $c.benchmark_group($op);
        add!(grp, "vbitmap", $n, $va);
        add!(grp, "fixedbitset", $n, $vb);
        add!(grp, $nc, $n, $vc);
        add!(grp, "bit-set", $n, $vd);
        add!(grp, "bitvec", $n, $ve);
        if $n == 256 {
            let m = bmaps(&bits);
            add!(grp, "bitmaps", $n, $vg(&m));
        }
        grp.finish();
    }};
}

fn count_ones(c: &mut Criterion) {
    for n in SIZES {
        unary!(
            c,
            "count_ones",
            make_bits(n, 4, 1),
            n,
            a,
            b,
            cc,
            d,
            e,
            black_box(&a).count_set_bits(),
            black_box(&b).count_ones(..),
            "bit-vec",
            black_box(&cc).count_ones(),
            black_box(&d).count(),
            black_box(&e).count_ones(),
            |m: &bitmaps::Bitmap<256>| black_box(m).len()
        );
    }
}

fn first_set(c: &mut Criterion) {
    for n in SIZES {
        let mut bits = vec![false; n];
        bits[n - 1] = true;
        unary!(
            c,
            "first_set",
            bits,
            n,
            a,
            b,
            cc,
            d,
            e,
            black_box(&a).first_set(),
            black_box(&b).minimum(),
            "bit-vec (iter().position)",
            black_box(&cc).iter().position(|x| x),
            black_box(&d).iter().next(),
            black_box(&e).first_one(),
            |m: &bitmaps::Bitmap<256>| black_box(m).first_index()
        );
    }
}

fn is_zero(c: &mut Criterion) {
    for n in SIZES {
        unary!(
            c,
            "is_zero",
            vec![false; n],
            n,
            a,
            b,
            cc,
            d,
            e,
            black_box(&a).is_zero(),
            black_box(&b).is_clear(),
            "bit-vec",
            black_box(&cc).none(),
            black_box(&d).is_empty(),
            black_box(&e).not_any(),
            |m: &bitmaps::Bitmap<256>| black_box(m).is_empty()
        );
    }
}

fn iterate(c: &mut Criterion) {
    for (op, density) in [("iter_ones_dense", 2u64), ("iter_ones_sparse", 64)] {
        for n in SIZES {
            unary!(
                c,
                op,
                make_bits(n, density, 3),
                n,
                a,
                b,
                cc,
                d,
                e,
                black_box(&a).iter_ones().sum::<usize>(),
                black_box(&b).ones().sum::<usize>(),
                "bit-vec (iter+filter)",
                black_box(&cc)
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| *x)
                    .map(|(i, _)| i)
                    .sum::<usize>(),
                black_box(&d).iter().sum::<usize>(),
                black_box(&e).iter_ones().sum::<usize>(),
                |m: &bitmaps::Bitmap<256>| black_box(m).into_iter().sum::<usize>()
            );
        }
    }
}

/// `dst = a op b` for every crate: clone the left operand, then operate in
/// place. `vbitmap` also has a fused form that does it in one pass.
fn logic(c: &mut Criterion) {
    macro_rules! logic_op {
        ($op:literal, $n:expr, $m1:ident, $mf:ident, $m2:ident, $m3:ident, $m4:ident, $sym:tt, $in:expr) => {{
            let (ba, bb) = $in;
            let (a1, b1) = (vb(&ba), vb(&bb));
            let (a2, b2) = (fb(&ba), fb(&bb));
            let (a3, b3) = (bvec(&ba), bvec(&bb));
            let (a4, b4) = (bset(&ba), bset(&bb));
            let (a5, b5) = (bvc(&ba), bvc(&bb));
            let mut x1 = NativeBitmap::new();
            let mut x2 = FixedBitSet::with_capacity($n);
            let mut x3 = BitVecCrate::<usize>::new_general();
            let mut x4 = BitSet::<usize>::default();
            let mut x5 = BitVec::<usize, Lsb0>::new();
            let mut xf = NativeBitmap::new();
            let mut g = c.benchmark_group($op);
            add!(g, "vbitmap", $n, {
                x1.copy_from(black_box(&a1));
                x1.$m1(black_box(&b1));
                black_box(x1.len_words())
            });
            add!(g, "vbitmap (fused, a op b into dst)", $n, {
                xf.$mf(black_box(&a1), black_box(&b1));
                black_box(xf.len_words())
            });
            add!(g, "fixedbitset", $n, {
                x2.clone_from(black_box(&a2));
                x2.$m2(black_box(&b2));
                black_box(x2.len())
            });
            add!(g, "bit-vec", $n, {
                x3.clone_from(black_box(&a3));
                x3.$m3(black_box(&b3));
                black_box(x3.len())
            });
            add!(g, "bit-set", $n, {
                x4.clone_from(black_box(&a4));
                x4.$m4(black_box(&b4));
                black_box(x4.get_ref().len())
            });
            add!(g, "bitvec", $n, {
                x5.clone_from(black_box(&a5));
                x5 $sym black_box(&b5);
                black_box(x5.len())
            });
            g.finish();
        }};
    }
    for n in SIZES {
        let inputs = || (make_bits(n, 2, 11), make_bits(n, 2, 12));
        logic_op!("and", n, and, and_from, intersect_with, and, intersect_with, &=, inputs());
        logic_op!("or", n, or, or_from, union_with, or, union_with, |=, inputs());
        logic_op!("xor", n, xor, xor_from, symmetric_difference_with, xor, symmetric_difference_with, ^=, inputs());
    }
}

fn get_batch(c: &mut Criterion) {
    for n in SIZES {
        let bits = make_bits(n, 2, 5);
        let ix = indices(n, 6);
        let (a, b, cc, d, e, g) = (
            vb(&bits),
            fb(&bits),
            bvec(&bits),
            bset(&bits),
            bvc(&bits),
            bvv(&bits),
        );
        let mut grp = c.benchmark_group("get_batch1024");
        add!(
            grp,
            "vbitmap",
            n,
            black_box(&ix)
                .iter()
                .map(|&i| a.get(i) as usize)
                .sum::<usize>()
        );
        add!(
            grp,
            "fixedbitset",
            n,
            black_box(&ix)
                .iter()
                .map(|&i| b.contains(i) as usize)
                .sum::<usize>()
        );
        add!(
            grp,
            "bit-vec",
            n,
            black_box(&ix)
                .iter()
                .map(|&i| cc.get(i).unwrap() as usize)
                .sum::<usize>()
        );
        add!(
            grp,
            "bit-set",
            n,
            black_box(&ix)
                .iter()
                .map(|&i| d.contains(i) as usize)
                .sum::<usize>()
        );
        add!(
            grp,
            "bitvec",
            n,
            black_box(&ix).iter().map(|&i| e[i] as usize).sum::<usize>()
        );
        add!(
            grp,
            "bv",
            n,
            black_box(&ix)
                .iter()
                .map(|&i| g.get_bit(i as u64) as usize)
                .sum::<usize>()
        );
        grp.finish();
    }
}

/// Alternates a pass that sets the bits with a pass that clears them, so every
/// crate really stores. (Repeating the same writes would reward crates that
/// skip the store when the bit already has the value, such as `bit-set`'s
/// `insert`.) The time is the average per pass.
fn write_batch(c: &mut Criterion) {
    for n in SIZES {
        let bits = make_bits(n, 2, 7);
        let ix = indices(n, 8);
        let (mut a, mut a4) = (vb(&bits), vb(&bits));
        let (mut b, mut cc, mut d, mut e, mut g) =
            (fb(&bits), bvec(&bits), bset(&bits), bvc(&bits), bvv(&bits));
        let mut v = false;
        let mut grp = c.benchmark_group("write_batch1024");
        add!(grp, "vbitmap (set)", n, {
            v = !v;
            for &i in black_box(&ix) {
                if v {
                    a.set(i);
                } else {
                    a.unset(i);
                }
            }
            black_box(a.len_words())
        });
        add!(grp, "vbitmap (BitSlice set/unset)", n, {
            v = !v;
            // borrow the words once: pointer and length stay in registers
            let s: &mut BitSlice<usize> = &mut a4;
            if v {
                for &i in black_box(&ix) {
                    s.set(i);
                }
            } else {
                for &i in black_box(&ix) {
                    s.unset(i);
                }
            }
            black_box(s.len_words())
        });
        add!(grp, "fixedbitset", n, {
            v = !v;
            for &i in black_box(&ix) {
                b.set(i, v);
            }
            black_box(b.len())
        });
        add!(grp, "bit-vec", n, {
            v = !v;
            for &i in black_box(&ix) {
                cc.set(i, v);
            }
            black_box(cc.len())
        });
        add!(grp, "bit-set", n, {
            v = !v;
            for &i in black_box(&ix) {
                if v {
                    d.insert(i);
                } else {
                    d.remove(i);
                }
            }
            black_box(d.get_ref().len())
        });
        add!(grp, "bitvec", n, {
            v = !v;
            for &i in black_box(&ix) {
                e.set(i, v);
            }
            black_box(e.len())
        });
        add!(grp, "bv", n, {
            v = !v;
            for &i in black_box(&ix) {
                g.set_bit(i as u64, v);
            }
            black_box(g.len())
        });
        grp.finish();
    }
}

fn set_range(c: &mut Criterion) {
    for n in SIZES {
        let len = n - 2;
        let mut a = NativeBitmap::alloc(n);
        let mut b = FixedBitSet::with_capacity(n);
        let mut e = BitVec::<usize, Lsb0>::repeat(false, n);
        let mut v = false;
        let mut grp = c.benchmark_group("set_range");
        add!(grp, "vbitmap", n, {
            v = !v;
            a.set_region(1, v, black_box(len))
        });
        add!(grp, "fixedbitset", n, {
            v = !v;
            b.set_range(1..black_box(len) + 1, v)
        });
        add!(grp, "bitvec (slice.fill)", n, {
            v = !v;
            e[1..black_box(len) + 1].fill(v)
        });
        grp.finish();
    }
}

fn get_field24(c: &mut Criterion) {
    for n in SIZES {
        let bits = make_bits(n, 2, 9);
        let ix = indices(n - 24, 10);
        let (a, e, g) = (vb(&bits), bvc(&bits), bvv(&bits));
        let mut grp = c.benchmark_group("get_field24_batch1024");
        add!(
            grp,
            "vbitmap",
            n,
            black_box(&ix)
                .iter()
                .fold(0u64, |s, &i| s.wrapping_add(a.get_field(i, 24)))
        );
        add!(
            grp,
            "bitvec (load_le)",
            n,
            black_box(&ix).iter().fold(0u64, |s, &i| {
                s.wrapping_add(e[i..i + 24].load_le::<u32>() as u64)
            })
        );
        add!(
            grp,
            "bv (get_bits)",
            n,
            black_box(&ix).iter().fold(0u64, |s, &i| {
                s.wrapping_add(g.get_bits(i as u64, 24) as u64)
            })
        );
        grp.finish();
    }
}

fn main() {
    if let Some(n) = std::env::var("LAYOUT_SHIFT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        std::mem::forget(vec![0u8; n]);
    }
    let mut c = Criterion::default()
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(1))
        .sample_size(50)
        .configure_from_args();
    count_ones(&mut c);
    first_set(&mut c);
    is_zero(&mut c);
    iterate(&mut c);
    logic(&mut c);
    get_batch(&mut c);
    write_batch(&mut c);
    set_range(&mut c);
    get_field24(&mut c);
    c.final_summary();
}
