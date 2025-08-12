// Jackson Coxson

use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_header(
            "// Jackson Coxson\n// Bindings to idevice - https://github.com/jkcoxson/idevice",
        )
        .with_language(cbindgen::Language::C)
        .with_sys_include("sys/socket.h")
        .with_include("plist.h")
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("idevice.h");

    // download plist.h
    let h = ureq::get("https://raw.githubusercontent.com/libimobiledevice/libplist/refs/heads/master/include/plist/plist.h")
        .call()
        .expect("failed to download plist.h");
    let h = h
        .into_body()
        .read_to_string()
        .expect("failed to get string content");
    std::fs::write("plist.h", h).expect("failed to save to string");

    println!("cargo:rustc-link-arg=-lplist-2.0");
}
