fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let svg = "../assets/Acord.svg";
        let ico = "icon.ico";
        let rc = "icon.rc";
        let res = "icon.res";
        let tmp = "icon_tmp";

        println!("cargo:rerun-if-changed={svg}");

        // Rasterize SVG → PNGs → ICO.
        let _ = std::fs::create_dir_all(tmp);
        let sizes = [16, 24, 32, 48, 64, 128, 256];
        let mut pngs = Vec::new();
        for size in sizes {
            let out = format!("{tmp}/icon_{size}.png");
            let s = size.to_string();
            if !run(&["rsvg-convert", "--width", &s, "--height", &s, svg, "-o", &out]) {
                println!("cargo:warning=rsvg-convert failed — no icon");
                let _ = std::fs::remove_dir_all(tmp);
                return;
            }
            pngs.push(out);
        }
        let mut magick_args: Vec<&str> = pngs.iter().map(|s| s.as_str()).collect();
        magick_args.push(ico);
        if !run_vec("magick", &magick_args) {
            println!("cargo:warning=magick failed — no icon");
            let _ = std::fs::remove_dir_all(tmp);
            return;
        }
        let _ = std::fs::remove_dir_all(tmp);

        // Write .rc and compile with llvm-windres directly.
        std::fs::write(rc, "1 ICON \"icon.ico\"\r\n").ok();
        if !run(&["llvm-windres", rc, "-o", res]) {
            println!("cargo:warning=llvm-windres failed — no icon");
            return;
        }

        // Tell the linker to include the compiled resource.
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-arg-bins={manifest_dir}/{res}");
        println!("cargo:warning=icon embedded via llvm-windres");
    }
}

#[cfg(target_os = "windows")]
fn run(args: &[&str]) -> bool {
    std::process::Command::new(args[0])
        .args(&args[1..])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn run_vec(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
