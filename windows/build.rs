use std::process::Command;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let svg = "../assets/Acord.svg";
        let ico = "icon.ico";
        let tmp = "icon_tmp";

        // Only regenerate on release builds or when the SVG changes.
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
                eprintln!("cargo:warning=rsvg-convert failed for {size}px — skipping icon embed");
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
            eprintln!("cargo:warning=magick ico conversion failed — skipping icon embed");
            return;
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon(ico);
        res.compile().expect("winres icon embed");
    }
}
