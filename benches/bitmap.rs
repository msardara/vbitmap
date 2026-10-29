// SPDX-License-Identifier: MIT OR Apache-2.0
//! Criterion benchmarks: `Bitmap<u64>` against `fixedbitset`, and the word
//! size trade-off (`u32` / `u64` / `u128`) for the scanning operations.
#![cfg(has_alloc)]

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use fixedbitset::FixedBitSet;
use vbitmap::{BitSlice, Bitmap, Word};

/// Bits: 4, 64 and 4096 u64 words.
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

/// Each bit set with probability 1/`density`; the top bit is always set so
/// the length is stable.
fn make_bits(n_bits: usize, density: u64, seed: u64) -> Vec<bool> {
    let mut r = Rng(seed);
    let mut v: Vec<bool> = (0..n_bits).map(|_| r.next() % density == 0).collect();
    *v.last_mut().unwrap() = true;
    v
}

fn bitmap<W: Word>(bits: &[bool]) -> Bitmap<W> {
    let mut b = Bitmap::<W>::alloc(bits.len());
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        b.set(i);
    }
    b
}

fn fixed(bits: &[bool]) -> FixedBitSet {
    let mut f = FixedBitSet::with_capacity(bits.len());
    for (i, _) in bits.iter().enumerate().filter(|(_, x)| **x) {
        f.insert(i);
    }
    f
}

fn indices(n_bits: usize, seed: u64) -> Vec<usize> {
    let mut r = Rng(seed);
    (0..BATCH)
        .map(|_| (r.next() % n_bits as u64) as usize)
        .collect()
}

fn bench_scan(c: &mut Criterion) {
    let mut g = c.benchmark_group("first_set_sparse");
    for &n in &SIZES {
        let mut bits = vec![false; n];
        bits[n - 1] = true; // worst case scan
        g.throughput(Throughput::Bytes((n / 8) as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            bn.iter(|| black_box(&b).first_set())
        });
        let b = bitmap::<u32>(&bits);
        g.bench_function(BenchmarkId::new("u32", n), |bn| {
            bn.iter(|| black_box(&b).first_set())
        });
        let b = bitmap::<u128>(&bits);
        g.bench_function(BenchmarkId::new("u128", n), |bn| {
            bn.iter(|| black_box(&b).first_set())
        });
        let f = fixed(&bits);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            bn.iter(|| black_box(&f).ones().next())
        });
    }
    g.finish();

    let mut g = c.benchmark_group("is_zero");
    for &n in &SIZES {
        let bits = vec![false; n];
        g.throughput(Throughput::Bytes((n / 8) as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            bn.iter(|| black_box(&b).is_zero())
        });
        let f = fixed(&bits);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            bn.iter(|| black_box(&f).is_clear())
        });
    }
    g.finish();

    let mut g = c.benchmark_group("count_set_bits");
    for &n in &SIZES {
        let bits = make_bits(n, 4, 1);
        g.throughput(Throughput::Bytes((n / 8) as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            bn.iter(|| black_box(&b).count_set_bits())
        });
        let b = bitmap::<u32>(&bits);
        g.bench_function(BenchmarkId::new("u32", n), |bn| {
            bn.iter(|| black_box(&b).count_set_bits())
        });
        let f = fixed(&bits);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            bn.iter(|| black_box(&f).count_ones(..))
        });
    }
    g.finish();

    let mut g = c.benchmark_group("last_set");
    for &n in &SIZES {
        let mut bits = vec![false; n];
        bits[0] = true;
        g.throughput(Throughput::Bytes((n / 8) as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            bn.iter(|| black_box(&b).last_set())
        });
    }
    g.finish();
}

fn bench_iterate(c: &mut Criterion) {
    for (name, density) in [("iter_ones_dense", 2u64), ("iter_ones_sparse", 64)] {
        let mut g = c.benchmark_group(name);
        for &n in &SIZES {
            let bits = make_bits(n, density, 3);
            let b = bitmap::<u64>(&bits);
            g.throughput(Throughput::Elements(b.count_set_bits() as u64));
            g.bench_function(BenchmarkId::new("iter_ones", n), |bn| {
                bn.iter(|| black_box(&b).iter_ones().map(|i| i as u64).sum::<u64>())
            });
            // restart the search after every bit instead of using the iterator
            g.bench_function(BenchmarkId::new("next_set_loop", n), |bn| {
                bn.iter(|| {
                    let b = black_box(&b);
                    let mut sum = 0u64;
                    let mut cur = b.first_set();
                    while let Some(i) = cur {
                        sum += i as u64;
                        cur = b.next_set(i + 1);
                    }
                    sum
                })
            });
            let f = fixed(&bits);
            g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
                bn.iter(|| black_box(&f).ones().map(|i| i as u64).sum::<u64>())
            });
        }
        g.finish();
    }
}

fn bench_alu(c: &mut Criterion) {
    macro_rules! alu {
        ($name:literal, $op:ident, $fixed:expr) => {{
            let mut g = c.benchmark_group($name);
            for &n in &SIZES {
                let (ba, bb) = (make_bits(n, 2, 11), make_bits(n, 2, 12));
                g.throughput(Throughput::Bytes((n / 8) as u64));
                let (a, b) = (bitmap::<u64>(&ba), bitmap::<u64>(&bb));
                let mut scratch = Bitmap::<u64>::new();
                g.bench_function(BenchmarkId::new("u64", n), |bn| {
                    bn.iter(|| {
                        scratch.copy_from(black_box(&a));
                        scratch.$op(black_box(&b));
                        scratch.len_words()
                    })
                });
                let (fa, fb) = (fixed(&ba), fixed(&bb));
                let mut fs = FixedBitSet::with_capacity(n);
                g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
                    bn.iter(|| {
                        fs.clone_from(black_box(&fa));
                        let f: fn(&mut FixedBitSet, &FixedBitSet) = $fixed;
                        f(&mut fs, black_box(&fb));
                        fs.len()
                    })
                });
            }
            g.finish();
        }};
    }
    alu!("and", and, |a, b| a.intersect_with(b));
    alu!("or", or, |a, b| a.union_with(b));
    alu!("xor", xor, |a, b| a.symmetric_difference_with(b));
}

fn bench_bits(c: &mut Criterion) {
    let mut g = c.benchmark_group("get_batch1024");
    for &n in &SIZES {
        let bits = make_bits(n, 2, 5);
        let idx = indices(n, 6);
        g.throughput(Throughput::Elements(BATCH as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("get", n), |bn| {
            bn.iter(|| {
                black_box(&idx)
                    .iter()
                    .map(|&i| b.get(i) as usize)
                    .sum::<usize>()
            })
        });
        let f = fixed(&bits);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            bn.iter(|| {
                black_box(&idx)
                    .iter()
                    .map(|&i| f.contains(i) as usize)
                    .sum::<usize>()
            })
        });
    }
    g.finish();

    let mut g = c.benchmark_group("set_batch1024");
    for &n in &SIZES {
        let bits = make_bits(n, 2, 7);
        let idx = indices(n, 8);
        g.throughput(Throughput::Elements(BATCH as u64));
        let mut b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("set", n), |bn| {
            bn.iter(|| {
                for &i in black_box(&idx) {
                    b.set(i);
                }
                b.len_words()
            })
        });
        let mut b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("set_many", n), |bn| {
            bn.iter(|| {
                b.set_many(black_box(&idx).iter().copied());
                b.len_words()
            })
        });
        let mut b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("slice_set", n), |bn| {
            bn.iter(|| {
                let s: &mut BitSlice<u64> = &mut b;
                for &i in black_box(&idx) {
                    s.set(i);
                }
                s.len_words()
            })
        });
        let mut f = fixed(&bits);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            bn.iter(|| {
                for &i in black_box(&idx) {
                    f.insert(i);
                }
                f.len()
            })
        });
    }
    g.finish();

    let mut g = c.benchmark_group("get_field_batch1024");
    for &n in &SIZES {
        let bits = make_bits(n, 2, 9);
        let idx = indices(n, 10);
        g.throughput(Throughput::Elements(BATCH as u64));
        let b = bitmap::<u64>(&bits);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            bn.iter(|| {
                black_box(&idx)
                    .iter()
                    .fold(0u64, |s, &i| s.wrapping_add(b.get_field(i, 24)))
            })
        });
    }
    g.finish();

    let mut g = c.benchmark_group("set_region");
    for &n in &SIZES {
        let len = n - 2;
        g.throughput(Throughput::Bytes((len / 8) as u64));
        let mut b = Bitmap::<u64>::alloc(n);
        g.bench_function(BenchmarkId::new("u64", n), |bn| {
            let mut v = false;
            bn.iter(|| {
                v = !v;
                b.set_region(1, v, black_box(len - 1));
            })
        });
        let mut f = FixedBitSet::with_capacity(n);
        g.bench_function(BenchmarkId::new("fixedbitset", n), |bn| {
            let mut v = false;
            bn.iter(|| {
                v = !v;
                f.set_range(1..black_box(len), v);
            })
        });
    }
    g.finish();
}

criterion_group!(benches, bench_scan, bench_iterate, bench_alu, bench_bits);
criterion_main!(benches);
