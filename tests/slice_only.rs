// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runs without the `alloc` feature (`cargo test --no-default-features`): only
//! the borrowed `BitSlice` API over plain arrays is available.

use core::fmt::Write;

use vbitmap::BitSlice;

struct Buf {
    data: [u8; 64],
    len: usize,
}

impl Write for Buf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        self.data[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}

impl Buf {
    fn new() -> Self {
        Buf {
            data: [0; 64],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap()
    }
}

#[test]
fn slice_over_array() {
    let mut words = [0u32; 4];
    let s = BitSlice::new_mut(&mut words);
    s.set(1);
    s.set(2);
    s.set(70);
    assert_eq!(s.first_set(), Some(1));
    assert_eq!(s.last_set(), Some(70));
    assert_eq!(s.count_set_bits(), 3);
    assert_eq!(s.next_set(3), Some(70));
    assert_eq!(s.next_clear(1), 3);
    assert_eq!(s.iter_ones().collect_into_buf(), 3);
}

trait CountOnes {
    fn collect_into_buf(self) -> usize;
}
impl<I: Iterator<Item = usize>> CountOnes for I {
    fn collect_into_buf(self) -> usize {
        self.count()
    }
}

#[test]
fn formatting_without_alloc() {
    let words = [0b1110_1111u64 << 4];
    let s = BitSlice::new(&words);
    let mut b = Buf::new();
    write!(b, "{s}").unwrap();
    assert_eq!(b.as_str(), "4-7, 9-11");
    let mut b = Buf::new();
    s.write_hex(&mut b).unwrap();
    assert_eq!(b.as_str(), "ef0");
}
