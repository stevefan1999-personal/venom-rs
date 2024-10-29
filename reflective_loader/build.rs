use std::path::Path;

fn main() {
    println!("cargo:rustc-link-arg=/ALIGN:16");
    // println!("cargo:rustc-link-arg=/MERGE:.rdata=.text");
    // println!("cargo:rustc-link-arg=/MERGE:.pdata=.text");
    println!("cargo:rustc-link-arg=/MERGE:.data=.text");
    println!("cargo:rustc-link-arg=/NOENTRY");
    println!("cargo:rustc-link-arg=/DEBUG:NONE");
    println!("cargo:rustc-link-arg=/EMITPOGOPHASEINFO");
    println!("cargo:rustc-link-arg=/EHsc");
    println!("cargo:rustc-link-arg=/SAFESEH:NO");
    println!("cargo:rustc-link-arg=/EMITTOOLVERSIONINFO:NO");
    println!("cargo:rustc-link-arg=/MANIFEST:NO");
    println!("cargo:rustc-link-arg=/NOVCFEATURE");
    println!("cargo:rustc-link-arg=/NOCOFFGRPINFO");
    println!("cargo:rustc-link-arg=/nologo");
    // See: https://github.com/mcountryman/min-sized-rust-windows/pull/7
    println!(
        "cargo:rustc-link-arg=/STUB:{}",
        Path::new("./stub.exe")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    println!(
        "cargo:rustc-link-arg=/DEF:{}",
        Path::new("./modules.def")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
}
