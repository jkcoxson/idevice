// Jackson Coxson

use std::{env, fs::OpenOptions, io::Write};

const HEADER: &str = r#"// Jackson Coxson
// Bindings to idevice - https://github.com/jkcoxson/idevice

#ifdef _WIN32
  #ifndef WIN32_LEAN_AND_MEAN
  #define WIN32_LEAN_AND_MEAN
  #endif
  #include <winsock2.h>
  #include <ws2tcpip.h>
  typedef int                idevice_socklen_t;
  typedef struct sockaddr    idevice_sockaddr;
#else
  #include <sys/types.h>
  #include <sys/socket.h>
  typedef socklen_t          idevice_socklen_t;
  typedef struct sockaddr    idevice_sockaddr;
#endif
"#;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_header(HEADER)
        .with_language(cbindgen::Language::C)
        .with_include_guard("IDEVICE_H")
        .exclude_item("idevice_socklen_t")
        .exclude_item("idevice_sockaddr")
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
    let mut f = OpenOptions::new().append(true).open("idevice.h").unwrap();
    f.write_all(b"\n\n\n").unwrap();
    f.write_all(&h.into_bytes())
        .expect("failed to append plist.h");

    let f = std::fs::read_to_string("idevice.h").unwrap();
    std::fs::write("../cpp/include/idevice.h", f).unwrap();
}
