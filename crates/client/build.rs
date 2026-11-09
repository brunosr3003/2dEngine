//! Windows only: embeds the app icon (windows/tempest.ico, generated from the
//! Google Play icon by tools/icone_app/gerar.py) into Tempest.exe, so
//! Explorer, the taskbar pin and the Start menu show the store icon.
//!
//! Needs MinGW's `windres` (scripts/build-windows.sh puts it on PATH). Without
//! it the build still works — the exe just has no icon — and says so.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=windows/tempest.ico");
    println!("cargo:rerun-if-changed=windows/tempest.rc");
    println!("cargo:rerun-if-env-changed=WINDRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows");
    let saida = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tempest_icone.o");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
    let ok = Command::new(&windres)
        .arg("--include-dir")
        .arg(&dir)
        .arg("-i")
        .arg(dir.join("tempest.rc"))
        .args(["-O", "coff", "-o"])
        .arg(&saida)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", saida.display());
    } else {
        println!("cargo:warning=windres not found ({windres}): Tempest.exe built without its icon");
    }
}
