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
        .with_sys_include("plist/plist.h")
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("idevice.h");
}
