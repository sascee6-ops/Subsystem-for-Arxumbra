fn main() {
    println!(
        "cargo:rustc-env=BIOS_IMG={}",
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("bios.img")
            .display()
    );
    bootloader::DiskImageBuilder::new(std::path::PathBuf::from(
        std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel").unwrap(),
    ))
    .create_bios_image(
        &std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("bios.img"),
    )
    .unwrap();
}
