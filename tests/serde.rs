//! Verify that the public serde feature supplies the serialization traits.
#![cfg(feature = "serde")]

use vbitmap::{ArrayBitmap, BitSlice};

#[test]
fn borrowed_and_fixed_bitmaps_use_trimmed_little_endian_bytes() {
    let words = [0x0102u16, 0];
    assert_eq!(
        serde_json::to_string(BitSlice::new(&words)).unwrap(),
        "[2,1]"
    );
    let fixed = ArrayBitmap::from_words(words);
    assert_eq!(serde_json::to_string(&fixed).unwrap(), "[2,1]");

    let decoded: ArrayBitmap<u8, 2> = serde_json::from_str("[2,1,0]").unwrap();
    assert_eq!(decoded.into_words(), [2, 1]);
    assert!(serde_json::from_str::<ArrayBitmap<u8, 2>>("[2,1,1]").is_err());
    let empty = ArrayBitmap::<u64, 0>::new();
    assert_eq!(serde_json::to_string(&empty).unwrap(), "[]");
}

#[cfg(has_alloc)]
#[test]
fn growable_bitmaps_round_trip_across_word_types() {
    use vbitmap::Bitmap;

    let bitmap = Bitmap::<u64>::from_words(vec![0x0102, 0]);
    let json = serde_json::to_string(&bitmap).unwrap();
    assert_eq!(json, "[2,1]");
    let narrow: Bitmap<u8> = serde_json::from_str(&json).unwrap();
    assert_eq!(narrow.as_words(), &[2, 1]);
    let wide: Bitmap<u128> = serde_json::from_str(&json).unwrap();
    assert_eq!(wide.as_words(), &[0x0102]);
}
