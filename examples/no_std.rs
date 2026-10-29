// SPDX-License-Identifier: MIT OR Apache-2.0
//! A real `#![no_std]` program: no `std`, no allocator, no `main` from
//! the Rust runtime. It keeps its bitmap in a `static` array, formats it into a
//! fixed byte buffer and checks the result; the exit code says whether
//! everything matched (0 = yes). Only `core` APIs of the crate are used.
//!
//! Run with `cargo run --example no_std --features no_std` and look at the
//! exit code (`echo $?`).
//!
//! On a microcontroller you would drop `main` and the platform link
//! attribute, keep the `static` and the logic, and send the buffer contents to
//! your serial port instead of comparing them.

#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

use vbitmap::BitSlice;

/// 4 x 64 = 256 flags, living in `.bss`: no allocator involved. On a target
/// with a different native word, use `[usize; N]`: `usize` is the machine's
/// native word and the best default; `u64` is
/// used here to keep the printed output the same everywhere.
static mut FLAGS: [u64; 4] = [0; 4];

// Hosted targets start a program through their system library; this empty block
// only makes the linker pull it in. Bare metal targets do not need it.
#[link(name = "c")]
unsafe extern "C" {}

/// A fixed-size text buffer: where a serial port or log would be on hardware.
struct Buf {
    data: [u8; 256],
    len: usize,
}

impl Write for Buf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        let dst = self.data.get_mut(self.len..end).ok_or(fmt::Error)?;
        dst.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl Buf {
    fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    // SAFETY: single threaded, and this is the only reference to FLAGS.
    // (Going through a raw pointer avoids the `static_mut_refs` error; clippy
    // would rewrite it into exactly that error.)
    #[allow(clippy::deref_addrof)]
    let words = unsafe { &mut *(&raw mut FLAGS) };
    let flags = BitSlice::new_mut(words);

    // Set a few flags. `BitSlice::set` never grows, so it cannot allocate.
    for i in [3, 4, 5, 6, 64, 65, 200] {
        flags.set(i);
    }

    let mut out = Buf {
        data: [0; 256],
        len: 0,
    };
    let ok = (|| -> Result<bool, fmt::Error> {
        write!(out, "{flags} | ")?; // range list through `Display`
        flags.write_hex(&mut out)?;
        write!(
            out,
            " | first={:?} last={:?} count={} next after 7 = {:?}",
            flags.first_set(),
            flags.last_set(),
            flags.count_set_bits(),
            flags.next_set(7),
        )?;
        let mut sum = 0;
        for bit in flags.iter_ones() {
            sum += bit;
        }
        write!(out, " | sum={sum}")?;

        // 4-bit field read across a word boundary.
        flags.set_field_strict(62, 0b1010, 4);
        write!(out, " | field={:#06b}", flags.get_field(62, 4))?;
        Ok(true)
    })();

    const EXPECTED: &[u8] = b"3-6, 64-65, 200 | 100000000000000000000000000000000030000000000000078 | first=Some(3) last=Some(200) count=7 next after 7 = Some(64) | sum=347 | field=0b1010";
    if ok == Ok(true) && out.as_bytes() == EXPECTED {
        0
    } else {
        1
    }
}

/// A `no_std` binary on a hosted target links `core` built with unwinding
/// support, which references this symbol even though panics abort. Bare metal
/// targets do not need it.
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
