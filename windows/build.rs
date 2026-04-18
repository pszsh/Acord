fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        let svg = "../assets/Acord.svg";
        let ico = "icon.ico";
        let tmp = "icon_tmp";

        println!("cargo:rerun-if-changed={svg}");

        let _ = std::fs::create_dir_all(tmp);
        let sizes = [16, 24, 32, 48, 64, 128, 256];
        let mut pngs = Vec::new();

        for size in sizes {
            let out = format!("{tmp}/icon_{size}.png");
            let s = size.to_string();
            let ok = Command::new("rsvg-convert")
                .args(["--width", &s, "--height", &s, svg, "-o", &out])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                eprintln!("cargo:warning=rsvg-convert not found or failed — building without icon");
                let _ = std::fs::remove_dir_all(tmp);
                return;
            }
            pngs.push(out);
        }

        let ok = Command::new("magick")
            .args(pngs.iter().map(|s| s.as_str()))
            .arg(ico)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let _ = std::fs::remove_dir_all(tmp);

        if !ok {
            eprintln!("cargo:warning=magick (ImageMagick) not found — building without icon");
            return;
        }

        println!("cargo:warning=icon.ico generated, embedding via winres...");
        let mut res = winres::WindowsResource::new();
        res.set_icon(ico);
        match res.compile() {
            Ok(_) => println!("cargo:warning=icon embedded successfully"),
            Err(e) => eprintln!("cargo:warning=winres failed: {e} — building without icon"),
        }
    }
}
