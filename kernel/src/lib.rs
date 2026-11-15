#![no_std]

pub fn boot(pixels: &[core::sync::atomic::AtomicU32], present: impl Fn(), halt: impl Fn() -> bool) {
    pixels.iter().for_each(|pixel| pixel.store(u32::MAX, core::sync::atomic::Ordering::Relaxed));
    present();
    while halt() {}
}
