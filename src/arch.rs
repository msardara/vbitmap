// SPDX-License-Identifier: MIT OR Apache-2.0
//! Architecture-specific kernels. Each one is optional: the generic code in
//! `slice.rs` is used on every other target.

/// Number of set bits in `bytes`, using NEON byte popcounts with four
/// independent accumulators. NEON is part of the aarch64 baseline, so no
/// runtime detection is needed.
#[cfg(target_arch = "aarch64")]
pub(crate) fn popcount_bytes(bytes: &[u8]) -> usize {
    use core::arch::aarch64::{vaddlvq_u16, vcntq_u8, vdupq_n_u16, vld1q_u8, vpadalq_u8};

    let chunks = bytes.chunks_exact(64);
    let mut total = 0usize;
    let mut left = chunks.len();
    let mut ptr = bytes.as_ptr();
    // Each vpadalq adds at most 16 to a u16 lane, so 2000 rounds cannot
    // overflow; fold the accumulators into `total` between blocks.
    while left > 0 {
        let rounds = left.min(2000);
        // SAFETY: `rounds * 64` bytes starting at `ptr` lie inside `bytes`
        // (they are whole 64-byte chunks of it), and vld1q_u8 has no
        // alignment requirement.
        unsafe {
            let (mut a0, mut a1, mut a2, mut a3) = (
                vdupq_n_u16(0),
                vdupq_n_u16(0),
                vdupq_n_u16(0),
                vdupq_n_u16(0),
            );
            for _ in 0..rounds {
                a0 = vpadalq_u8(a0, vcntq_u8(vld1q_u8(ptr)));
                a1 = vpadalq_u8(a1, vcntq_u8(vld1q_u8(ptr.add(16))));
                a2 = vpadalq_u8(a2, vcntq_u8(vld1q_u8(ptr.add(32))));
                a3 = vpadalq_u8(a3, vcntq_u8(vld1q_u8(ptr.add(48))));
                ptr = ptr.add(64);
            }
            total +=
                (vaddlvq_u16(a0) + vaddlvq_u16(a1) + vaddlvq_u16(a2) + vaddlvq_u16(a3)) as usize;
        }
        left -= rounds;
    }
    for &b in chunks.remainder() {
        total += b.count_ones() as usize;
    }
    total
}

/// Number of set bits in `op(a, b)`, byte by byte, with the loop of
/// [`popcount_bytes`]. `a` and `b` have the same length.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub(crate) fn popcount_bytes2(
    a: &[u8],
    b: &[u8],
    op: impl Fn(
        core::arch::aarch64::uint8x16_t,
        core::arch::aarch64::uint8x16_t,
    ) -> core::arch::aarch64::uint8x16_t,
) -> usize {
    use core::arch::aarch64::{vaddlvq_u16, vcntq_u8, vdupq_n_u16, vld1q_u8, vpadalq_u8};

    debug_assert_eq!(a.len(), b.len());
    let ca = a.chunks_exact(64);
    let cb = b.chunks_exact(64);
    let mut total = 0usize;
    let mut left = ca.len();
    let (mut pa, mut pb) = (a.as_ptr(), b.as_ptr());
    while left > 0 {
        let rounds = left.min(2000);
        // SAFETY: `rounds * 64` bytes from `pa` and `pb` lie inside `a` and
        // `b` (whole 64-byte chunks), and vld1q_u8 has no alignment
        // requirement.
        unsafe {
            let (mut a0, mut a1, mut a2, mut a3) = (
                vdupq_n_u16(0),
                vdupq_n_u16(0),
                vdupq_n_u16(0),
                vdupq_n_u16(0),
            );
            for _ in 0..rounds {
                a0 = vpadalq_u8(a0, vcntq_u8(op(vld1q_u8(pa), vld1q_u8(pb))));
                a1 = vpadalq_u8(a1, vcntq_u8(op(vld1q_u8(pa.add(16)), vld1q_u8(pb.add(16)))));
                a2 = vpadalq_u8(a2, vcntq_u8(op(vld1q_u8(pa.add(32)), vld1q_u8(pb.add(32)))));
                a3 = vpadalq_u8(a3, vcntq_u8(op(vld1q_u8(pa.add(48)), vld1q_u8(pb.add(48)))));
                pa = pa.add(64);
                pb = pb.add(64);
            }
            total +=
                (vaddlvq_u16(a0) + vaddlvq_u16(a1) + vaddlvq_u16(a2) + vaddlvq_u16(a3)) as usize;
        }
        left -= rounds;
    }
    let (ra, rb) = (ca.remainder(), cb.remainder());
    if !ra.is_empty() {
        // the tail, through a zero-padded 64-byte buffer so the same kernel
        // finishes it
        let (mut x, mut y) = ([0u8; 64], [0u8; 64]);
        x[..ra.len()].copy_from_slice(ra);
        y[..rb.len()].copy_from_slice(rb);
        total += popcount_bytes2(&x, &y, op);
    }
    total
}

/// Index (0 to 3) of the first non-zero word among the four 64-bit words at
/// `ptr`, or `None` if all four are zero: one 256-bit load, a compare against
/// zero and a movemask, with no per-word branches. Compiled only when AVX2 is
/// enabled for the whole build (for example `-C target-cpu=native`).
///
/// # Safety
/// `ptr` must be valid for reading four `u64`s (32 bytes, any alignment).
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline(always)]
pub(crate) unsafe fn first_nonzero_lane_of_4(ptr: *const u64) -> Option<usize> {
    use core::arch::x86_64::{
        __m256i, _mm256_castsi256_pd, _mm256_cmpeq_epi64, _mm256_loadu_si256, _mm256_movemask_pd,
        _mm256_setzero_si256,
    };
    // SAFETY: the caller guarantees 32 readable bytes; the intrinsics need
    // AVX2, which this function is compiled with.
    unsafe {
        let v = _mm256_loadu_si256(ptr.cast::<__m256i>());
        let zero_lanes = _mm256_movemask_pd(_mm256_castsi256_pd(_mm256_cmpeq_epi64(
            v,
            _mm256_setzero_si256(),
        ))) as u32;
        let nonzero = !zero_lanes & 0xF;
        if nonzero == 0 {
            None
        } else {
            Some(nonzero.trailing_zeros() as usize)
        }
    }
}

/// Are the `n` (4 to 8) 64-bit words at `ptr` all zero? Two overlapping
/// 256-bit loads cover them; OR them and test once. Compiled only when AVX2
/// is enabled for the whole build.
///
/// # Safety
/// `ptr` must be valid for reading `n` `u64`s, and `4 <= n <= 8`.
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline(always)]
pub(crate) unsafe fn all_zero_4_to_8(ptr: *const u64, n: usize) -> bool {
    use core::arch::x86_64::{__m256i, _mm256_loadu_si256, _mm256_or_si256, _mm256_testz_si256};
    // SAFETY: the caller guarantees words `0..n` are readable with `n >= 4`,
    // so both `0..4` and `n - 4..n` are; the intrinsics need AVX2, which this
    // function is compiled with.
    unsafe {
        let lo = _mm256_loadu_si256(ptr.cast::<__m256i>());
        let hi = _mm256_loadu_si256(ptr.add(n - 4).cast::<__m256i>());
        let v = _mm256_or_si256(lo, hi);
        _mm256_testz_si256(v, v) != 0
    }
}
