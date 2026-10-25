//! Names the configurations the sources test: `native` when the target's
//! platform feature is enabled, and `access` when its system accessibility
//! adapter is too. A new platform adds one row here.

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let feature = |name: &str| std::env::var_os(format!("CARGO_FEATURE_{name}")).is_some();
    // (target OS, platform feature, accessibility adapter feature)
    let platforms = [
        ("linux", "WAYLAND", "UNIX_ACCESSIBILITY"),
        ("windows", "WINDOWS", "WINDOWS_ACCESSIBILITY"),
    ];
    println!("cargo::rustc-check-cfg=cfg(native, access)");
    for (target, platform, access) in platforms {
        if os == target && feature(platform) {
            println!("cargo::rustc-cfg=native");
            if feature(access) {
                println!("cargo::rustc-cfg=access");
            }
        }
    }
}
