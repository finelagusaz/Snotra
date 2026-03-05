fn main() {
    // Ensure sidecar placeholder exists so tauri_build doesn't fail during
    // cargo check / clippy. The real binary is built by scripts/prepare-sidecar.ps1.
    let sidecar = std::path::Path::new("binaries/snotra-settings-x86_64-pc-windows-msvc.exe");
    if !sidecar.exists() {
        if let Some(parent) = sidecar.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::File::create(sidecar);
    }

    tauri_build::build()
}
