fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    cbindgen::generate(&crate_dir)
        .expect("cbindgen failed")
        .write_to_file(format!("{}/include/acord.h", crate_dir));
}
