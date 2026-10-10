use std::{env, fs, path::PathBuf, process::Command};

use image::{ExtendedColorType, codecs::ico::{IcoEncoder, IcoFrame}, imageops};

fn main() {
    println!("cargo:rerun-if-changed=assets/artlauncher.png");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let source = image::open("assets/artlauncher.png").expect("Launcher logo").to_rgba8();
    let (mut left, mut top, mut right, mut bottom) = (source.width(), source.height(), 0, 0);
    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel[3] > 16 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    assert!(left <= right && top <= bottom, "Launcher logo is empty");
    let left = left.saturating_sub(2);
    let top = top.saturating_sub(2);
    let right = (right + 3).min(source.width());
    let bottom = (bottom + 3).min(source.height());
    let cropped = imageops::crop_imm(&source, left, top, right - left, bottom - top).to_image();
    let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256].into_iter().map(|size| {
        let inner = ((size as f32 * 0.86).round() as u32).max(1);
        let logo = imageops::resize(&cropped, inner, inner, imageops::FilterType::Lanczos3);
        let mut scaled = image::RgbaImage::new(size, size);
        let offset = ((size - inner) / 2) as i64;
        imageops::overlay(&mut scaled, &logo, offset, offset);
        IcoFrame::as_png(scaled.as_raw(), size, size, ExtendedColorType::Rgba8).expect("Icon frame")
    }).collect();
    let icon = out.join("CraftLauncher.ico");
    IcoEncoder::new(fs::File::create(&icon).expect("Icon file"))
        .encode_images(&frames).expect("Encode Windows icon");
    let resource = out.join("launcher.rc");
    let version = env::var("CARGO_PKG_VERSION").expect("Package version");
    let numbers = version.split('.').collect::<Vec<_>>().join(",");
    let version_info = format!(r#"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "CraftLauncher"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "CraftLauncher"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "CraftLauncher.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#);
    fs::write(&resource, format!("1 ICON \"{}\"\n{version_info}", icon.display().to_string().replace('\\', "/")))
        .expect("Icon resource source");

    let sdk = env::var_os("WindowsSdkDir").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK location"))
            .join("Windows Kits/10")
    });
    let mut versions: Vec<_> = fs::read_dir(sdk.join("bin")).expect("Install the Windows SDK")
        .filter_map(|entry| entry.ok().map(|entry| entry.path())).collect();
    versions.sort();
    let rc = versions.into_iter().rev().map(|dir| dir.join("x64/rc.exe"))
        .find(|path| path.is_file()).expect("Windows SDK resource compiler (rc.exe)");
    let compiled = out.join("launcher.res");
    let status = Command::new(rc).arg("/nologo").arg("/fo").arg(&compiled).arg(&resource)
        .status().expect("Run Windows resource compiler");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=CraftLauncher={}", compiled.display());
}
