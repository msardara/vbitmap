# Benchmarks

`vbitmap` against the most downloaded bitmap and bitset crates on crates.io
(by downloads, search "bitmap" and "bitset": `fixedbitset`, `bit-vec`, `bit-set`,
`bitvec`, `bitmaps`, `bv`). Same data in every crate, same operations,
one Criterion harness: `benches/compare.rs` (run it with `just compare`).

## How to read the tables

Each cell is the median time of one call in nanoseconds. The ratio in parentheses
is that crate's time divided by vbitmap's, so above 1.00 means slower than vbitmap;
compare ratios across machines, not nanoseconds.

* **Result:** of 72 measurements (12 operations, 3 sizes, 2 machines)
  vbitmap is clearly ahead of the best dense crate in 60 and level with it in
  11 (within about 5%); 1 cells are behind by more than 5%.
* **Method:** Criterion medians, and the minimum over 3 runs with different
  heap layouts (`LAYOUT_SHIFT`). Differences under about 5% are ties.
* **Placement noise:** rows of a few nanoseconds move 10 to 30% with where the
  compiler puts a loop or the allocator puts a small bitmap, for any crate.
* **Not like for like:** `bitmaps` only exists up to 1024 bits, so only at 256;
  `bit-vec` has no search or set-bit iterator (measured as `iter().position(..)`
  and `iter().enumerate().filter(..)`). `bit-vec` and `bit-set` use `usize`
  blocks (their default, `u32`, is slower).
* Some vbitmap fast paths are per architecture (see the README).

## Machines

* arm64 (Apple M4), macOS, rustc 1.94.0, default release flags (no target-cpu=native), not pinned
* x86-64 (Intel Xeon 6737P, Granite Rapids) in a Linux VM, one core pinned with taskset, -C target-cpu=native


## arm64 (Apple M4)

### Population count

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **0.9** | **4.5** | **347** |
| bit-set | 1.5 (1.61) | 6.1 (1.36) | 382 (1.10) |
| bit-vec | 1.5 (1.60) | 6.1 (1.36) | 381 (1.10) |
| bitmaps | 0.6 (0.67) | n/a | n/a |
| bitvec | 2.7 (3.00) | 6.6 (1.46) | 387 (1.11) |
| fixedbitset | 1.5 (1.61) | 6.2 (1.37) | 380 (1.10) |

### First set bit (worst case: only the last bit is set)

`bit-vec` has no search, so it is measured as `iter().position(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.1** | **5.6** | **202** |
| bit-set | 1.0 (0.95) | 16.3 (2.89) | 970 (4.80) |
| bit-vec (iter().position) | 84.6 (79.48) | 1,097 (194.54) | 64,297 (317.91) |
| bitmaps | 0.6 (0.61) | n/a | n/a |
| bitvec | 2.8 (2.63) | 19.9 (3.53) | 1,119 (5.53) |
| fixedbitset | 1.1 (1.03) | 16.4 (2.90) | 1,048 (5.18) |

### Is the bitmap all zero

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.0** | **3.2** | **192** |
| bit-set | 1.4 (1.37) | 15.5 (4.76) | 927 (4.83) |
| bit-vec | 1.4 (1.38) | 15.6 (4.80) | 928 (4.83) |
| bitmaps | 0.4 (0.40) | n/a | n/a |
| bitvec | 2.8 (2.66) | 6.6 (2.04) | 389 (2.03) |
| fixedbitset | 1.5 (1.39) | 15.8 (4.86) | 958 (4.99) |

### Iterate set bits, half of the bits set

`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **48.4** | **748** | **50,233** |
| bit-set | 61.7 (1.27) | 1,188 (1.59) | 77,560 (1.54) |
| bit-vec (iter+filter) | 92.4 (1.91) | 1,419 (1.90) | 90,609 (1.80) |
| bitmaps | 408 (8.41) | n/a | n/a |
| bitvec | 815 (16.83) | 12,258 (16.38) | 788,768 (15.70) |
| fixedbitset | 56.9 (1.17) | 1,202 (1.61) | 79,449 (1.58) |

### Iterate set bits, 1 in 64 set

`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.5** | **24.2** | **2,727** |
| bit-set | 1.6 (1.08) | 27.6 (1.14) | 3,632 (1.33) |
| bit-vec (iter+filter) | 92.6 (62.09) | 1,427 (58.85) | 90,629 (33.24) |
| bitmaps | 4.5 (3.01) | n/a | n/a |
| bitvec | 13.5 (9.08) | 365 (15.05) | 25,774 (9.45) |
| fixedbitset | 2.0 (1.33) | 27.5 (1.13) | 3,681 (1.35) |

### In-place AND (clone the left operand, then AND)

`vbitmap` is clone then in-place AND, like every other crate. `vbitmap (fused)` computes `dst = a & b` in one pass (`and_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 3.9 (1.93) | 15.7 (1.74) | 656 (1.41) |
| vbitmap (fused, a op b into dst) | **2.0** | **9.0** | **464** |
| bit-set | 5.3 (2.61) | 41.1 (4.57) | 2,233 (4.81) |
| bit-vec | 4.0 (1.94) | 13.4 (1.50) | 649 (1.40) |
| bitvec | 30.1 (14.74) | 337 (37.53) | 20,468 (44.12) |
| fixedbitset | 4.9 (2.41) | 16.3 (1.81) | 635 (1.37) |

### In-place OR (clone, then OR)

`vbitmap` is clone then in-place OR, like every other crate. `vbitmap (fused)` computes `dst = a | b` in one pass (`or_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 4.1 (1.39) | 15.4 (1.51) | 656 (1.33) |
| vbitmap (fused, a op b into dst) | **3.0** | **10.2** | **494** |
| bit-set | 5.4 (1.80) | 41.1 (4.04) | 2,245 (4.54) |
| bit-vec | 4.0 (1.33) | 13.5 (1.33) | 650 (1.32) |
| bitvec | 30.1 (10.10) | 336 (33.07) | 20,425 (41.35) |
| fixedbitset | 4.7 (1.58) | 15.3 (1.50) | 626 (1.27) |

### In-place XOR (clone, then XOR)

`vbitmap` is clone then in-place XOR, like every other crate. `vbitmap (fused)` computes `dst = a ^ b` in one pass (`xor_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 4.0 (1.33) | 15.4 (1.56) | 656 (1.36) |
| vbitmap (fused, a op b into dst) | **3.0** | **9.9** | **481** |
| bit-set | 5.4 (1.83) | 41.3 (4.18) | 2,239 (4.65) |
| bit-vec | 4.0 (1.33) | 13.4 (1.36) | 657 (1.36) |
| bitvec | 30.0 (10.08) | 341 (34.46) | 20,562 (42.73) |
| fixedbitset | 4.7 (1.58) | 15.5 (1.57) | 626 (1.30) |

### 1024 random bit reads

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **260** | **259** | **259** |
| bit-set | 341 (1.32) | 342 (1.32) | 342 (1.32) |
| bit-vec | 344 (1.32) | 344 (1.33) | 344 (1.32) |
| bitvec | 335 (1.29) | 336 (1.30) | 337 (1.30) |
| bv | 325 (1.25) | 324 (1.25) | 324 (1.25) |
| fixedbitset | 261 (1.01) | 261 (1.01) | 261 (1.01) |

### 1024 random bit writes (alternating a set pass and a clear pass)

The reference is `vbitmap (BitSlice set/unset)` (called on a `BitSlice` borrowed once, as a hot loop would): bounds-checked, never grows, like the `set` of `fixedbitset`, `bit-vec`, `bitvec` and `bv`, which all panic on an out-of-range index. `vbitmap (set)` grows the bitmap on demand and is the like-for-like row for `bit-set`, which also grows. Alternating passes make every crate really store: `bit-set`'s `insert`/`remove` skip the store when the bit already has the value, so repeating the same write would flatter it.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap (BitSlice set/unset) | **606** | **344** | **286** |
| vbitmap (set) | 894 (1.48) | 520 (1.51) | 485 (1.70) |
| bit-set | 867 (1.43) | 1,057 (3.08) | 796 (2.79) |
| bit-vec | 992 (1.64) | 556 (1.62) | 554 (1.94) |
| bitvec | 987 (1.63) | 559 (1.63) | 553 (1.94) |
| bv | 952 (1.57) | 575 (1.67) | 575 (2.01) |
| fixedbitset | 1,020 (1.68) | 421 (1.22) | 387 (1.36) |

### Set or clear a range of bits

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.7** | **5.8** | **356** |
| bitvec (slice.fill) | 3.7 (2.11) | 8.8 (1.52) | 482 (1.36) |
| fixedbitset | 2.1 (1.19) | 22.7 (3.91) | 1,412 (3.97) |

### 1024 random 24-bit field reads

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **442** | **372** | **345** |
| bitvec (load_le) | 2,076 (4.70) | 2,158 (5.80) | 2,104 (6.09) |
| bv (get_bits) | 769 (1.74) | 805 (2.16) | 801 (2.32) |


## x86-64 (Intel Xeon 6737P

### Population count

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.5** | **4.3** | **258** |
| bit-set | 1.5 (1.03) | 4.3 (1.00) | 259 (1.00) |
| bit-vec | 1.5 (1.03) | 4.3 (1.01) | 259 (1.00) |
| bitmaps | 1.0 (0.69) | n/a | n/a |
| bitvec | 4.3 (2.96) | 6.4 (1.50) | 260 (1.01) |
| fixedbitset | 1.8 (1.21) | 4.3 (1.00) | 258 (1.00) |

### First set bit (worst case: only the last bit is set)

`bit-vec` has no search, so it is measured as `iter().position(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **1.3** | **5.9** | **209** |
| bit-set | 1.7 (1.31) | 26.5 (4.46) | 1,039 (4.97) |
| bit-vec (iter().position) | 234 (181.27) | 3,612 (607.37) | 230,524 (1101.29) |
| bitmaps | 1.2 (0.89) | n/a | n/a |
| bitvec | 4.0 (3.12) | 32.7 (5.50) | 1,534 (7.33) |
| fixedbitset | 2.1 (1.66) | 8.0 (1.34) | 402 (1.92) |

### Is the bitmap all zero

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **0.8** | **3.7** | **205** |
| bit-set | 2.1 (2.80) | 27.3 (7.42) | 1,126 (5.50) |
| bit-vec | 2.1 (2.83) | 27.3 (7.42) | 1,126 (5.50) |
| bitmaps | 0.5 (0.72) | n/a | n/a |
| bitvec | 4.4 (5.89) | 6.5 (1.76) | 260 (1.27) |
| fixedbitset | 1.2 (1.56) | 7.0 (1.90) | 401 (1.96) |

### Iterate set bits, half of the bits set

`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **42.5** | **780** | **78,006** |
| bit-set | 63.5 (1.49) | 1,128 (1.45) | 81,120 (1.04) |
| bit-vec (iter+filter) | 169 (3.98) | 2,581 (3.31) | 164,632 (2.11) |
| bitmaps | 562 (13.23) | n/a | n/a |
| bitvec | 923 (21.70) | 12,258 (15.72) | 790,260 (10.13) |
| fixedbitset | 77.8 (1.83) | 1,201 (1.54) | 77,932 (1.00) |

### Iterate set bits, 1 in 64 set

`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **2.3** | **35.0** | **2,872** |
| bit-set | 2.3 (1.00) | 49.2 (1.40) | 3,889 (1.35) |
| bit-vec (iter+filter) | 169 (74.79) | 2,582 (73.67) | 164,641 (57.34) |
| bitmaps | 6.9 (3.04) | n/a | n/a |
| bitvec | 17.3 (7.67) | 438 (12.49) | 31,101 (10.83) |
| fixedbitset | 2.9 (1.30) | 73.4 (2.09) | 13,638 (4.75) |

### In-place AND (clone the left operand, then AND)

`vbitmap` is clone then in-place AND, like every other crate. `vbitmap (fused)` computes `dst = a & b` in one pass (`and_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 3.4 (1.34) | 9.3 (1.47) | 1,102 (1.53) |
| vbitmap (fused, a op b into dst) | **2.5** | **6.3** | **721** |
| bit-set | 7.3 (2.91) | 48.3 (7.61) | 2,643 (3.66) |
| bit-vec | 4.0 (1.59) | 11.4 (1.79) | 1,263 (1.75) |
| bitvec | 42.4 (16.86) | 559 (88.20) | 34,266 (47.49) |
| fixedbitset | 4.5 (1.80) | 11.0 (1.74) | 1,176 (1.63) |

### In-place OR (clone, then OR)

`vbitmap` is clone then in-place OR, like every other crate. `vbitmap (fused)` computes `dst = a | b` in one pass (`or_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 3.5 (1.27) | 9.2 (1.45) | 1,104 (1.53) |
| vbitmap (fused, a op b into dst) | **2.8** | **6.3** | **722** |
| bit-set | 5.4 (1.96) | 42.7 (6.75) | 2,659 (3.68) |
| bit-vec | 4.0 (1.44) | 11.8 (1.87) | 1,300 (1.80) |
| bitvec | 41.6 (15.10) | 549 (86.77) | 32,735 (45.34) |
| fixedbitset | 4.8 (1.74) | 11.1 (1.75) | 1,123 (1.56) |

### In-place XOR (clone, then XOR)

`vbitmap` is clone then in-place XOR, like every other crate. `vbitmap (fused)` computes `dst = a ^ b` in one pass (`xor_from`), which no other crate offers; the reference for the ratios is the faster of the two.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | 3.3 (1.12) | 9.1 (1.40) | 1,108 (1.53) |
| vbitmap (fused, a op b into dst) | **3.0** | **6.5** | **726** |
| bit-set | 5.3 (1.78) | 42.5 (6.52) | 2,670 (3.68) |
| bit-vec | 3.9 (1.31) | 11.3 (1.74) | 1,284 (1.77) |
| bitvec | 43.3 (14.61) | 564 (86.67) | 35,017 (48.22) |
| fixedbitset | 4.8 (1.62) | 11.0 (1.69) | 1,129 (1.56) |

### 1024 random bit reads

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **233** | **233** | **234** |
| bit-set | 651 (2.79) | 651 (2.80) | 652 (2.79) |
| bit-vec | 651 (2.79) | 651 (2.80) | 652 (2.79) |
| bitvec | 522 (2.24) | 522 (2.24) | 524 (2.24) |
| bv | 517 (2.22) | 517 (2.22) | 518 (2.21) |
| fixedbitset | 235 (1.01) | 235 (1.01) | 235 (1.01) |

### 1024 random bit writes (alternating a set pass and a clear pass)

The reference is `vbitmap (BitSlice set/unset)` (called on a `BitSlice` borrowed once, as a hot loop would): bounds-checked, never grows, like the `set` of `fixedbitset`, `bit-vec`, `bitvec` and `bv`, which all panic on an out-of-range index. `vbitmap (set)` grows the bitmap on demand and is the like-for-like row for `bit-set`, which also grows. Alternating passes make every crate really store: `bit-set`'s `insert`/`remove` skip the store when the bit already has the value, so repeating the same write would flatter it.

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap (BitSlice set/unset) | **504** | **530** | **406** |
| vbitmap (set) | 630 (1.25) | 559 (1.06) | 550 (1.36) |
| bit-set | 1,034 (2.05) | 1,175 (2.22) | 1,123 (2.77) |
| bit-vec | 805 (1.60) | 794 (1.50) | 816 (2.01) |
| bitvec | 1,017 (2.02) | 1,016 (1.92) | 1,018 (2.51) |
| bv | 774 (1.54) | 784 (1.48) | 688 (1.70) |
| fixedbitset | 633 (1.26) | 627 (1.18) | 609 (1.50) |

### Set or clear a range of bits

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **2.0** | **5.3** | **138** |
| bitvec (slice.fill) | 5.6 (2.80) | 8.3 (1.57) | 434 (3.14) |
| fixedbitset | 4.1 (2.08) | 52.0 (9.87) | 2,669 (19.31) |

### 1024 random 24-bit field reads

| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |
|---|---|---|---|
| vbitmap | **788** | **792** | **792** |
| bitvec (load_le) | 3,478 (4.41) | 3,561 (4.49) | 3,600 (4.54) |
| bv (get_bits) | 848 (1.08) | 858 (1.08) | 861 (1.09) |
