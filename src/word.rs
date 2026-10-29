// SPDX-License-Identifier: MIT OR Apache-2.0
//! The machine word a bitmap is built from.

use core::fmt;
use core::hash::Hash;
use core::ops::{BitAnd, BitOr, BitXor, Not, Shl, Shr};

mod private {
    pub trait Sealed {}
}

/// Unsigned integer usable as bitmap storage: `u8`, `u16`, `u32`, `u64`,
/// `u128` and `usize`. Sealed; every method is an `#[inline(always)]`
/// forward to the primitive so generic code compiles to the same
/// instructions as hand written code for the concrete type.
///
/// # Examples
///
/// Generic code can work for any word type:
///
/// ```
/// use vbitmap::{BitSlice, Word};
///
/// fn highest_bit<W: Word>(words: &[W]) -> Option<usize> {
///     BitSlice::new(words).last_set()
/// }
///
/// assert_eq!(highest_bit(&[1u8, 0b100]), Some(10));
/// assert_eq!(highest_bit(&[1u64 << 63]), Some(63));
/// assert_eq!(<u16 as Word>::BITS, 16);
/// assert_eq!(<u8 as Word>::low_mask(3), 0b111);
/// ```
pub trait Word:
    private::Sealed
    + Copy
    + Eq
    + Hash
    + Default
    + fmt::LowerHex
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Not<Output = Self>
    + Shl<usize, Output = Self>
    + Shr<usize, Output = Self>
    + 'static
{
    /// Bits per word.
    const BITS: usize;
    /// All bits clear.
    const ZERO: Self;
    /// Only the lowest bit set (the value 1).
    const ONE: Self;
    /// All bits set.
    const ONES: Self;

    /// Number of set bits.
    fn count_ones(self) -> u32;
    /// Number of clear bits below the lowest set bit (`BITS` for zero).
    fn trailing_zeros(self) -> u32;
    /// Number of clear bits above the highest set bit (`BITS` for zero).
    fn leading_zeros(self) -> u32;
    /// Reverse the order of bits in this word.
    fn reverse_bits(self) -> Self;
    /// `self - rhs`, wrapping on underflow.
    fn wrapping_sub(self, rhs: Self) -> Self;
    /// Truncating conversion from `u64`.
    fn from_u64(v: u64) -> Self;
    /// Truncating conversion to `u64`.
    fn to_u64(self) -> u64;

    /// The `n` low bits set; all bits for `n >= BITS`.
    #[inline(always)]
    fn low_mask(n: usize) -> Self {
        if n >= Self::BITS {
            Self::ONES
        } else {
            !(Self::ONES << n)
        }
    }

    /// `self` with its lowest set bit cleared (`self & (self - 1)`).
    #[inline(always)]
    fn clear_lowest_set(self) -> Self {
        self & self.wrapping_sub(Self::ONE)
    }

    /// Only bit `i` (`i < BITS`) set.
    #[inline(always)]
    fn bit(i: usize) -> Self {
        Self::ONE << i
    }
}

macro_rules! impl_word {
    ($($t:ty),*) => {$(
        impl private::Sealed for $t {}
        impl Word for $t {
            const BITS: usize = <$t>::BITS as usize;
            const ZERO: Self = 0;
            const ONE: Self = 1;
            const ONES: Self = !0;
            #[inline(always)] fn count_ones(self) -> u32 { <$t>::count_ones(self) }
            #[inline(always)] fn trailing_zeros(self) -> u32 { <$t>::trailing_zeros(self) }
            #[inline(always)] fn leading_zeros(self) -> u32 { <$t>::leading_zeros(self) }
            #[inline(always)] fn reverse_bits(self) -> Self { <$t>::reverse_bits(self) }
            #[inline(always)] fn wrapping_sub(self, rhs: Self) -> Self { <$t>::wrapping_sub(self, rhs) }
            #[inline(always)] fn from_u64(v: u64) -> Self { v as $t }
            #[inline(always)] fn to_u64(self) -> u64 { self as u64 }
        }
    )*};
}

impl_word!(u8, u16, u32, u64, u128, usize);
