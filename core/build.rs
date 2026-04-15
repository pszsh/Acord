fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    let config = cbindgen::Config::from_file("cbindgen.toml")
        .unwrap_or_default();

    match cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
    {
        Ok(bindings) => {
            let path = format!("{}/include/acord.h", crate_dir);
            bindings.write_to_file(&path);
            println!("cargo:warning=cbindgen: wrote {}", path);
        }
        Err(e) => {
            println!("cargo:warning=cbindgen: {}", e);
        }
    }
}
