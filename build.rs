//! Embeds the application icon and version information into the executables.
//!
//! On MSVC this uses `rc.exe` from the Windows SDK (available on GitHub's
//! `windows-latest` runners). On other toolchains a resource compiler may be
//! missing; the build then continues without an embedded icon instead of failing.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/aeternavault.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    embed_windows_resources();
}

#[cfg(windows)]
fn embed_windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon/aeternavault.ico")
        .set("ProductName", "AeternaVault")
        .set("FileDescription", "AeternaVault")
        .set("OriginalFilename", "aeternavault.exe")
        .set("InternalName", "AeternaVault")
        .set("LegalCopyright", "MIT OR Apache-2.0")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);

    if let Err(err) = res.compile() {
        println!("cargo:warning=Windows resources (icon, version info) were not embedded: {err}");
    }
}
