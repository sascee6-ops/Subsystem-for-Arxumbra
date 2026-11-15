#![no_std]

pub fn boot(pixels: &[core::sync::atomic::AtomicU32], width: usize, present: impl Fn(), halt: impl Fn() -> bool) {
    pixels.iter().enumerate().for_each(|(index, pixel)| {
        pixel.store(
            if (index % width / 48 + index / width / 48) % 2 == 0 { 0xFFFF_FFFF } else { 0xFFE6_E6E6 },
            core::sync::atomic::Ordering::Relaxed,
        )
    });
    present();
    while halt() {}
}
