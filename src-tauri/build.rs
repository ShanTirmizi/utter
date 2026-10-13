fn main() {
    link_clang_runtime();
    tauri_build::build()
}

/// whisper.cpp's Metal backend uses Objective-C `@available` checks. When the
/// app targets an older macOS than the one it is built on, those checks call a
/// helper that lives in clang's runtime library, which Rust does not link by
/// default. Without this the release build fails at link time with
/// "___isPlatformVersionAtLeast" undefined.
fn link_clang_runtime() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(output) = std::process::Command::new("xcrun")
        .args(["clang", "--print-search-dirs"])
        .output()
    else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(dir) = text
        .lines()
        .find_map(|line| line.strip_prefix("libraries: ="))
        .and_then(|dirs| dirs.split(':').next())
    else {
        return;
    };
    println!("cargo:rustc-link-search=native={dir}/lib/darwin");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
