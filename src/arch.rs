// SPDX-License-Identifier: MIT OR Apache-2.0
//! Architecture-aware kernels with portable fallbacks for bitmap operations.

use crate::Word;

/// First non-zero word and its index. Long scans test eight words at a time.
#[inline]
pub(crate) fn first_nonzero<W: Word>(words: &[W]) -> Option<(usize, W)> {
    let mut i = 0;
    if words.len() >= 8 {
        while i + 8 <= words.len() {
            let chunk = &words[i..i + 8];
            let any = chunk[0]
                | chunk[1]
                | chunk[2]
                | chunk[3]
                | chunk[4]
                | chunk[5]
                | chunk[6]
                | chunk[7];
            if any != W::ZERO {
                break;
            }
            i += 8;
        }
    }
    while i < words.len() {
        let word = words[i];
        if word != W::ZERO {
            return Some((i, word));
        }
        i += 1;
    }
    None
}

/// First word that is not all ones, with its index.
#[inline]
pub(crate) fn first_not_ones<W: Word>(words: &[W]) -> Option<(usize, W)> {
    let mut i = 0;
    if words.len() >= 8 {
        while i + 8 <= words.len() {
            let chunk = &words[i..i + 8];
            let all = chunk[0]
                & chunk[1]
                & chunk[2]
                & chunk[3]
                & chunk[4]
                & chunk[5]
                & chunk[6]
                & chunk[7];
            if all != W::ONES {
                break;
            }
            i += 8;
        }
    }
    while i < words.len() {
        let word = words[i];
        if word != W::ONES {
            return Some((i, word));
        }
        i += 1;
    }
    None
}

/// Length of a word slice without trailing zero words.
#[inline]
pub(crate) fn nonzero_len<W: Word>(words: &[W]) -> usize {
    if words.len() < 8 {
        return words
            .iter()
            .rposition(|&word| word != W::ZERO)
            .map_or(0, |p| p + 1);
    }
    let mut end = words.len();
    while end >= 8 {
        let chunk = &words[end - 8..end];
        let any =
            chunk[0] | chunk[1] | chunk[2] | chunk[3] | chunk[4] | chunk[5] | chunk[6] | chunk[7];
        if any != W::ZERO {
            break;
        }
        end -= 8;
    }
    while end > 0 && words[end - 1] == W::ZERO {
        end -= 1;
    }
    end
}

/// Keep a loaded word in a register before a single-bit update.
///
/// LLVM otherwise folds the update into a memory-destination instruction on
/// x86-64. The empty asm constraint makes the load and store explicit without
/// emitting an instruction.
#[inline(always)]
pub(crate) fn in_register<W: Word>(word: W) -> W {
    #[cfg(target_arch = "x86_64")]
    if W::BITS == 64 {
        let mut value = word.to_u64();
        // SAFETY: the empty template only constrains `value` to a register.
        unsafe {
            core::arch::asm!(
                "/* {value} */",
                value = inout(reg) value,
                options(pure, nomem, nostack, preserves_flags)
            );
        }
        return W::from_u64(value);
    }
    word
}

/// Set a bit in an in-bounds word slice.
#[inline(always)]
pub(crate) fn set_bit<W: Word>(words: &mut [W], i: usize) {
    let few = words.len() <= 8;
    let word = &mut words[i / W::BITS];
    let new = in_register(*word) | W::bit(i % W::BITS);
    #[cfg(target_arch = "aarch64")]
    if few && new == *word {
        return;
    }
    #[cfg(not(target_arch = "aarch64"))]
    let _ = few;
    *word = new;
}

/// Clear a bit in an in-bounds word slice.
#[inline(always)]
pub(crate) fn unset_bit<W: Word>(words: &mut [W], i: usize) {
    let few = words.len() <= 8;
    let word = &mut words[i / W::BITS];
    let new = in_register(*word) & !W::bit(i % W::BITS);
    #[cfg(target_arch = "aarch64")]
    if few && new == *word {
        return;
    }
    #[cfg(not(target_arch = "aarch64"))]
    let _ = few;
    *word = new;
}

/// Read a bit, returning `false` when it is past the end.
#[inline(always)]
pub(crate) fn get<W: Word>(words: &[W], i: usize) -> bool {
    #[cfg(target_arch = "x86_64")]
    if words.len() <= usize::MAX / W::BITS {
        if i >= words.len() * W::BITS {
            return false;
        }
        // SAFETY: `i < len * BITS`, so the containing word exists.
        let word = unsafe { *words.get_unchecked(i / W::BITS) };
        return (word >> (i % W::BITS)) & W::ONE != W::ZERO;
    }
    match words.get(i / W::BITS) {
        Some(&word) => (word >> (i % W::BITS)) & W::ONE != W::ZERO,
        None => false,
    }
}

/// Is every word zero, using an AVX2 reduction when available.
#[inline]
pub(crate) fn is_zero<W: Word>(words: &[W]) -> bool {
    if words.len() < 8 {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        if W::BITS == 64 && words.len() >= 4 {
            // SAFETY: at least four words are readable.
            return unsafe { all_zero_4_to_8(words.as_ptr().cast::<u64>(), words.len()) };
        }
        return words.iter().all(|&word| word == W::ZERO);
    }
    let mut chunks = words.chunks_exact(8);
    for chunk in &mut chunks {
        let any =
            chunk[0] | chunk[1] | chunk[2] | chunk[3] | chunk[4] | chunk[5] | chunk[6] | chunk[7];
        if any != W::ZERO {
            return false;
        }
    }
    chunks.remainder().iter().all(|&word| word == W::ZERO)
}

/// Lowest set bit, using the target's preferred short scan and SIMD path.
#[inline(always)]
pub(crate) fn first_set<W: Word>(words: &[W]) -> Option<usize> {
    #[cfg(target_arch = "aarch64")]
    if words.len() >= 8 {
        return first_set_long(words);
    }
    #[cfg(not(target_arch = "aarch64"))]
    if words.len() >= 8 {
        let (p, word) = first_nonzero(words)?;
        return Some(p * W::BITS + word.trailing_zeros() as usize);
    }
    #[cfg(target_arch = "aarch64")]
    {
        let mut offset = 0;
        let mut pairs = words.chunks_exact(2);
        for pair in &mut pairs {
            if (pair[0] | pair[1]) != W::ZERO {
                let (word, skip) = if pair[0] != W::ZERO {
                    (pair[0], 0)
                } else {
                    (pair[1], W::BITS)
                };
                return Some(offset + skip + word.trailing_zeros() as usize);
            }
            offset += 2 * W::BITS;
        }
        match pairs.remainder() {
            [word] if *word != W::ZERO => Some(offset + word.trailing_zeros() as usize),
            _ => None,
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let mut offset = 0;
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        let rest = if W::BITS == 64 && words.len() >= 4 {
            // SAFETY: at least four words (32 bytes) are readable.
            if let Some(lane) = unsafe { first_nonzero_lane_of_4(words.as_ptr().cast::<u64>()) } {
                return Some(lane * 64 + words[lane].trailing_zeros() as usize);
            }
            offset = 4 * W::BITS;
            &words[4..]
        } else {
            words
        };
        #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
        let rest = words;
        for &word in rest {
            if word != W::ZERO {
                return Some(offset + word.trailing_zeros() as usize);
            }
            offset += W::BITS;
        }
        None
    }
}

#[cfg(target_arch = "aarch64")]
#[cold]
#[inline(never)]
fn first_set_long<W: Word>(words: &[W]) -> Option<usize> {
    let (p, word) = first_nonzero(words)?;
    Some(p * W::BITS + word.trailing_zeros() as usize)
}

/// Count set bits, using NEON byte popcounts for a large aarch64 bitmap.
#[inline]
pub(crate) fn count_set_bits<W: Word>(words: &[W]) -> usize {
    #[cfg(target_arch = "aarch64")]
    if words.len() >= 8 {
        // SAFETY: integer bytes are valid and `u8` has no alignment needs.
        let bytes = unsafe {
            core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), core::mem::size_of_val(words))
        };
        return popcount_bytes(bytes);
    }
    words.iter().map(|word| word.count_ones() as usize).sum()
}

/// Fill `n` words with a byte pattern, avoiding a page-split store on x86-64.
///
/// # Safety
/// `dst` must be writable for `n` words.
#[cfg(has_alloc)]
#[inline(always)]
pub(crate) unsafe fn fill_words<W: Word>(dst: *mut W, byte: u8, n: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        const PAGE: usize = 4096;
        let bytes = n * core::mem::size_of::<W>();
        let start = dst as usize;
        let split = (start | (PAGE - 1)) + 1;
        if bytes <= PAGE && split < start + bytes {
            let head = (split - start) / core::mem::size_of::<W>();
            // SAFETY: the split is word aligned and both parts are within `n`.
            unsafe {
                core::ptr::write_bytes(dst, byte, head);
                core::ptr::write_bytes(dst.add(head), byte, n - head);
            }
            return;
        }
    }
    // SAFETY: forwarded from the caller.
    unsafe { core::ptr::write_bytes(dst, byte, n) };
}

/// Fill the words strictly between the two edge words of a bit range.
#[cfg(has_alloc)]
#[inline(always)]
pub(crate) fn fill_region<W: Word>(
    words: &mut [W],
    first: usize,
    first_offset: usize,
    last: usize,
    last_offset: usize,
    value: bool,
) {
    #[inline(always)]
    fn apply<W: Word>(word: &mut W, mask: W, value: bool) {
        if value {
            *word = *word | mask;
        } else {
            *word = *word & !mask;
        }
    }

    apply(&mut words[first], W::ONES << first_offset, value);
    #[cfg(target_arch = "x86_64")]
    apply(
        &mut words[last],
        W::ONES >> (W::BITS - last_offset - 1),
        value,
    );

    let fill = if value { W::ONES } else { W::ZERO };
    match &mut words[first + 1..last] {
        [] => {}
        [a] => *a = fill,
        [a, b] => {
            *a = fill;
            *b = fill;
        }
        [a, b, c] => {
            *a = fill;
            *b = fill;
            *c = fill;
        }
        [a, b, c, d] => {
            *a = fill;
            *b = fill;
            *c = fill;
            *d = fill;
        }
        middle => {
            let byte = if value { 0xFF } else { 0 };
            // SAFETY: the middle slice is valid and exclusively borrowed.
            unsafe { fill_words(middle.as_mut_ptr(), byte, middle.len()) };
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    apply(
        &mut words[last],
        W::ONES >> (W::BITS - last_offset - 1),
        value,
    );
}

/// Copy a tail, skipping the empty memcpy on x86-64 where it costs more than
/// the short equal-length combine loop.
#[cfg(has_alloc)]
#[inline(always)]
pub(crate) fn copy_tail<T: Copy>(dst: &mut [T], src: &[T]) {
    #[cfg(target_arch = "x86_64")]
    if dst.is_empty() {
        return;
    }
    dst.copy_from_slice(src);
}

/// Position of the `n`th set bit in `x`; the caller guarantees it exists.
#[inline(always)]
pub(crate) fn select_u64(x: u64, n: usize) -> usize {
    #[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
    {
        // SAFETY: the caller guarantees `n < x.count_ones()` and BMI2 is enabled.
        return unsafe { core::arch::x86_64::_pdep_u64(1 << n, x) }.trailing_zeros() as usize;
    }
    #[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
    {
        select_u64_portable(x, n)
    }
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
fn select_u64_portable(x: u64, n: usize) -> usize {
    const L8: u64 = 0x0101_0101_0101_0101;
    const H8: u64 = 0x8080_8080_8080_8080;
    let mut s = x - ((x >> 1) & 0x5555_5555_5555_5555);
    s = (s & 0x3333_3333_3333_3333) + ((s >> 2) & 0x3333_3333_3333_3333);
    s = (s + (s >> 4)) & 0x0f0f_0f0f_0f0f_0f0f;
    let cumulative = s.wrapping_mul(L8);
    let before = (((n as u64 * L8) | H8) - cumulative) & H8;
    let byte = ((before >> 7).wrapping_mul(L8) >> 56) as usize;
    let below = ((cumulative << 8) >> (8 * byte)) as usize & 0xff;
    let bits = (x >> (8 * byte)) as usize & 0xff;
    8 * byte + SELECT_IN_BYTE[bits * 8 + (n - below)] as usize
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
static SELECT_IN_BYTE: [u8; 2048] = {
    let mut table = [0u8; 2048];
    let mut byte = 0;
    while byte < 256 {
        let mut rank = 0;
        let mut position = 0;
        while position < 8 {
            if byte & (1 << position) != 0 {
                table[byte * 8 + rank] = position as u8;
                rank += 1;
            }
            position += 1;
        }
        byte += 1;
    }
    table
};

/// Hide a value from LLVM's reassociation on x86-64; emits no instruction.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub(crate) fn opaque(mut value: usize) -> usize {
    // SAFETY: an empty asm block that only names a register.
    unsafe {
        core::arch::asm!(
            "/* {0} */",
            inout(reg) value,
            options(pure, nomem, nostack, preserves_flags)
        )
    };
    value
}

/// Align a hot loop to a measured offset within a 64-byte fetch block.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub(crate) fn place_loop<const OFFSET: usize>() {
    // SAFETY: jumps over padding that is never executed.
    unsafe {
        core::arch::asm!(
            "jmp 2f",
            ".p2align 6, 0xcc",
            ".skip {offset}, 0xcc",
            "2:",
            offset = const OFFSET,
            options(nomem, nostack, preserves_flags)
        )
    };
}

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

/// Byte-wise operation supported by the aarch64 pair-count kernel.
#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy)]
pub(crate) enum ByteOp {
    And,
    Or,
    Xor,
    AndNot,
}

/// Number of set bits in `op(a, b)`, byte by byte, with the loop of
/// [`popcount_bytes`]. `a` and `b` have the same length.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
pub(crate) fn popcount_bytes2(a: &[u8], b: &[u8], op: ByteOp) -> usize {
    use core::arch::aarch64::{vandq_u8, vbicq_u8, veorq_u8, vorrq_u8};

    match op {
        ByteOp::And => popcount_bytes2_with(a, b, |x, y| {
            // SAFETY: NEON is part of the aarch64 baseline.
            unsafe { vandq_u8(x, y) }
        }),
        ByteOp::Or => popcount_bytes2_with(a, b, |x, y| {
            // SAFETY: NEON is part of the aarch64 baseline.
            unsafe { vorrq_u8(x, y) }
        }),
        ByteOp::Xor => popcount_bytes2_with(a, b, |x, y| {
            // SAFETY: NEON is part of the aarch64 baseline.
            unsafe { veorq_u8(x, y) }
        }),
        ByteOp::AndNot => popcount_bytes2_with(a, b, |x, y| {
            // SAFETY: NEON is part of the aarch64 baseline.
            unsafe { vbicq_u8(x, y) }
        }),
    }
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn popcount_bytes2_with(
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
        total += popcount_bytes2_with(&x, &y, op);
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
