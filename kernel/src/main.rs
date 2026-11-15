#![no_std]
#![no_main]

bootloader_api::entry_point!(kernel);

fn kernel(info: impl core::ops::DerefMut<Target = bootloader_api::BootInfo>) -> ! {
    Some(info).iter_mut().for_each(|info| {
        info.framebuffer.as_mut().unwrap().buffer_mut().fill(255);
    });
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
