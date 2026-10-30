# vbitmap

Growable bitmap over generic machine words. `no_std`, no dependencies.

- `Bitmap<W>`: owned, grows on demand (`W` = `u8`..`u128`, `usize`; default `usize`).
  Prefer `NativeBitmap` (`Bitmap<usize>`): `usize` is the machine's native word
  (64 bits on 64-bit targets, 32 on 32-bit ones), so every
  step handles a full register. Use another word type only for a fixed layout.
- `BitSlice<W>`: a view over a slice of words. The queries and the updates that
  don't resize are its methods. A `Bitmap` derefs to one, and you can build one
  over a `&[W]`, an array or a buffer you don't own, without copying.
- Everything is generic over the sealed `Word` trait and `#[inline]`; each
  instantiation compiles to the code you would write for that concrete type.

```rust
use vbitmap::NativeBitmap;

let mut b = NativeBitmap::from_list("0-3,5-7,11").unwrap();
b.set(100);
assert_eq!(b.first_set(), Some(0));
assert_eq!(b.count_set_bits(), 9);
assert_eq!(b.format_list(), "0-3, 5-7, 11, 100");
for bit in b.iter_ones() { /* ... */ }
```

Bit `i` is bit `i % W::BITS` (LSB first) of word `i / W::BITS`. The bulk and
word-level operations that can clear bits (`and`, `andnot`, `xor`, `andi`,
`andnoti`, `xori`) trim trailing zero words, the `*_notrim` variants do not, and
`set` never shrinks the bitmap (call `trim`). Equality, hashing and all queries
ignore trailing zero words.

## Use cases

* **Which slots of a pool are in use.** Objects named by an index live in a
  table; the bitmap says which indices are taken. `first_clear` finds the lowest
  free slot, `set` takes it (the bitmap grows when the pool is full), `unset`
  gives it back, and `iter_ones` visits only the live objects.
* **CPU sets and other masks.** Parse and print the usual list notation
  (`0-3,8`), intersect with `and`, count with `count_set_bits`, pick the next
  one with `next_set`.
* **Dirty tracking.** Mark pages with `set` or `set_region`, flush with
  `iter_ones`, reset with `zero`.

The crate documentation has a working example of each.

## How it compares

Feature comparison with the most downloaded bitmap and bitset crates
(versions: `fixedbitset` 0.5.7, `bit-vec` 0.10.1, `bit-set` 0.11.1, `bitvec`
1.1.1, `bitmaps` 3.2.1, `bv` 0.11.1). Checked against each
crate's source and docs.

| | vbitmap | fixedbitset | bit-vec | bit-set | bitvec | bitmaps | bv |
|---|---|---|---|---|---|---|---|
| Kind | dense, growable | dense, fixed length (`grow`) | dense, growable | dense, growable set | dense, growable | dense, fixed size (up to 1024 bits) | dense, growable |
| `no_std` | yes | yes | yes | yes | yes | yes | no |
| Choose the word type | yes (`u8`..`u128`, `usize`) | no (`usize`) | yes (`u8`..`usize`) | yes | yes (any store, `Lsb0`/`Msb0`) | no | yes (`u8`..`u128`, `usize`) |
| Borrowed view over foreign memory | yes (`BitSlice`) | no | no | no | yes (`BitSlice`) | no | yes (`BitSlice`) |
| Iterate set bits | yes | yes | no (every bit) | yes | yes | yes | no |
| Next set / clear bit from a position | yes | no | no | no | no | yes | no |
| First / last set bit | yes | yes | no | no | yes | yes | no |
| Population count | yes | yes (ranged) | yes | yes | yes | yes | no |
| Set or clear a range | yes | yes | no | no | yes (slice `fill`) | no | no |
| Multi-bit field read/write | yes (up to 64 bits) | no | no | no | yes (`BitField`) | no | yes (`get_bits`) |
| In-place and / or / xor | yes | yes | yes | yes | yes (operators) | yes (operators) | yes |
| And-not (difference) | yes | yes | yes | yes | no | no | no |
| Range-list and hex formatting and parsing | yes | no | no | no | no | hex output only | no |
| Equality ignores length (set equality) | yes | no | no | yes | no | n/a | no |
| `serde` | yes | yes | yes | yes | yes | no | yes |

So the pitch is breadth in one small `no_std` crate: word-type choice, borrowed
views, searching from a position, ranges, fields and text, with performance
that matches or beats the specialised crates. Across the 72 measurements below
(12 operations at 3 sizes on 2 machines) it is clearly ahead of the best dense
crate in 60 and level with it in the other 12 (within about 5%, below what the
benchmarks can separate from code and heap placement); it is never more than
5% behind, and the one cell at 0.95 is `first_set` of a 256-bit bitmap on arm64. See [BENCHMARKS.md](BENCHMARKS.md) for every row against all of them on
x86-64 and arm64.

Summary of those benchmarks against the dense crates (`fixedbitset`,
`bit-vec`, `bit-set`, `bitvec`, `bv`), 12 operations at 3 sizes each. The
tables in [BENCHMARKS.md](BENCHMARKS.md) give nanoseconds per call and the
ratio to vbitmap; see there for how they are measured.

`bitmaps` supports fixed-size bitmaps up to 1,024 bits. Our benchmark sizes are
256, 4,096 and 262,144 bits, so only the 256-bit case fits; its larger-size
results are marked `n/a`.

| | vbitmap fastest (others at least 1.05x slower) | within 5% | 5 to 10% slower | more than 10% slower |
|---|---|---|---|---|
| arm64 (Apple M4) | 32 of 36 | 3 | 1 | 0 |
| x86-64 (Xeon 6737P) | 28 of 36 | 8 | 0 | 0 |

For and/or/xor the reference is the faster of `vbitmap`'s clone-then-operate
row (the same two steps every other crate needs) and its fused `and_from` /
`or_from` / `xor_from` (`dst = a op b` in one pass, which no other crate has);
for writes it is `set` / `unset` on a borrowed `BitSlice`
(a hot loop takes `let s: &mut BitSlice = &mut bitmap;` once).

Largest wins (nearest dense competitor vs vbitmap, best size on either machine):
`first_set` 4.8x, range updates 3.1x, 24-bit field reads 2.3x (arm64; 1.1x on
x86-64), `is_zero` 2.0x, and/or/xor 1.9x (fused), population count 1.6x, dense
iteration 1.6x, single-bit writes 1.5x, sparse iteration 1.4x. The crate docs
carry the same figures as a table.

On arm64 the three cells that are level are random reads at all three sizes
(every crate compiles to the same load-bound loop, about a quarter of a
nanosecond per read). The one cell behind is `first_set` of a 256-bit bitmap,
at 0.95 against `bit-set`; it was 0.97 in an earlier build that differed only
in unrelated code, so it is within what placement alone moves.

On x86-64 no cell is behind. The eight cells that are not clearly ahead are
ties: random reads at all three sizes, population count at all three sizes,
sparse iteration of 256 bits, and dense iteration of 262144 bits.

Some x86-64 specifics matter, found by reading the compiled code on the Xeon.
`first_set` and `is_zero` on a short bitmap use 256-bit loads when AVX2 is
enabled at compile time (for example `-C target-cpu=native`). `iter_ones().sum()`
and the other folds pass the bit position through an empty-assembly barrier so
that LLVM cannot put the word base on the accumulator's dependency chain, and
the short fold loop is pinned at a fixed offset inside a 64-byte block with a
jumped-over padding, because the same loop ran almost twice as slow wherever it
straddled a fetch block. Single-bit writes keep the word in a register
the same way, so that LLVM emits `bts` / `btr` and a plain store instead of a
memory-destination rotate and `and`. `get` compares the index with the length in
bits, which lets the compiler build the in-range mask of a vectorized read loop
without waiting for the shift. `Bitmap::set_region` splits its fill at a 4 KiB
page boundary, because a page-split store from glibc's `memset` doubled the cost
of a 4096-bit range on this CPU. Rows of a few nanoseconds also move with code
and heap placement: see [BENCHMARKS.md](BENCHMARKS.md).

The x86 build otherwise runs generic code: the NEON population count and the
dense-word decoder are aarch64 only.

What makes it fast, and what it costs:

* **64-byte aligned storage.** `Bitmap` keeps its words on a cache line, which
  makes bulk operations about 15% faster than a `Vec`. `from_words` and
  `into_words` therefore copy.
* **`set_many` and the borrowed `BitSlice`.** A growable `set(i)` through
  `&mut Bitmap` reloads the bitmap's pointer and length on every call. In hot
  loops use `set_many`, or borrow a `&mut BitSlice` once and call its `set` /
  `unset`. Neither shrinks the bitmap (call `trim`).
  On aarch64, for bitmaps of up to 8 words, `BitSlice::set` / `unset` skip the
  store when nothing changes, which avoids store-forwarding stalls when many
  writes hit the same word; larger bitmaps, and x86-64, always store.
* **Fused `and_from` / `or_from` / `xor_from` / `andnot_from`.**
* **`get_field`** reads a field of up to 56 bits with one unaligned 8-byte
  load on little-endian targets, whichever words it straddles.
* **`iter_ones`** skips empty words eight at a time when folding (`sum`,
  `for_each`, `collect`), and `first_set` does the same on long bitmaps.
* **aarch64 only:** population count uses NEON byte popcounts with four
  accumulators (bitmaps of 8 words or more), and `iter_ones().sum()` / `for_each`
  / `collect` decode dense 64-bit words as two independent halves, because
  clearing the lowest set bit is a two-instruction dependency per bit there.
  Other targets use the generic code.

## Features

- none (default): uses `std`; everything is available.
- `no_std`: the crate is `#![no_std]` and only the borrowed `BitSlice` API,
  `Ones` and `Word` remain (formatting through `write_hex` / `write_list` /
  `Display`, which take a `core::fmt::Write`).
- `alloc` (with `no_std`): adds `Bitmap`, `format_hex`, `format_list` and the
  parsers on top of the toolchain's `alloc` crate (nothing to install); the
  final binary needs a `#[global_allocator]`.

## Examples

`examples/` has one runnable example per area; each asserts what it shows.

| example | covers |
|---|---|
| `basic` | construction, single bits, storage management, equality |
| `search` | `first_set`/`last_set`/`next_*`/`first_clear`, `count_set_bits`, iteration |
| `logic` | and/or/xor/andnot, `dup_*`, operators, single-bit immediates |
| `fields` | multi-bit fields, regions, `set_many`, `random` |
| `text` | list and hex formatting and parsing, `Display`, `ParseError` |
| `words` | every word type, the `Word` trait, borrowed `BitSlice` views |
| `no_std` | a real `#![no_std]` program with a `static` array, no allocator |

```sh
cargo run --example basic
cargo run --example no_std --features no_std
just examples          # run them all
```

## Tests and benchmarks

```sh
just check                        # all quality gates, including coverage and audit
just test                         # tests and doctests for every feature combination
just test-native                  # optimized native CPU tests
just bench                        # Criterion benchmarks
just compare                      # vs fixedbitset, bit-vec, bit-set, bitvec, bitmaps, bv
RUSTFLAGS="-C target-cpu=native" just compare
```

## Releases

Pull requests run `.github/workflows/ci.yml`. Pushes to `main` run the same CI
through `.github/workflows/release-plz.yml`; publishing waits for all checks to
pass. CI checks formatting, Clippy, docs and packaging, uses `cargo hack` to test
every feature combination on Linux, macOS and Windows, checks Rust 1.85 and a
bare-metal target, and runs optimized native CPU tests on Linux and macOS.
All CI checks call recipes in `justfile`, so the same commands run locally.
Install `just` and `cargo-hack` to run the feature checks; `just check` also
requires `cargo-llvm-cov` and `cargo-audit` for its coverage and audit recipes.

After the first manual `cargo publish`, configure the following once:

1. In GitHub **Settings → Actions → General**, enable **Allow GitHub Actions to
   create and approve pull requests**.
2. In the crate's crates.io settings, add a Trusted Publisher for owner
   `msardara`, repository `vbitmap`, workflow `release-plz.yml`, with no
   environment restriction. The publish job uses OIDC; a
   `CARGO_REGISTRY_TOKEN` secret is unnecessary.
3. For CI to start automatically on release PRs, add a repository secret
   `RELEASE_PLZ_TOKEN` containing a fine-grained GitHub token scoped to this
   repository with **Contents: read/write** and **Pull requests: read/write**.
   Without it, release-plz uses `GITHUB_TOKEN`, which cannot trigger PR checks;
   close and reopen the release PR to trigger them. Main-branch CI still runs
   before publishing in either case.

release-plz opens a PR containing the next version and changelog. Review and
merge that PR to publish the crate and create its GitHub tag and release.
Ordinary pushes to `main` prepare release PRs without publishing new versions.

See the [release-plz setup guide](https://release-plz.dev/docs/github/quickstart)
and [GitHub token guidance](https://release-plz.dev/docs/github/token).

## License

MIT OR Apache-2.0
