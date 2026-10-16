/// Reads `codename = "..."` from `[package.metadata.callephiin]` in Cargo.toml.
fn codename_from_manifest() -> String {
    let text = std::fs::read_to_string("Cargo.toml").unwrap_or_default();
    let mut in_section = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l == "[package.metadata.callephiin]";
        } else if in_section && l.starts_with("codename") {
            if let Some(v) = l.split('=').nth(1) {
                return v.split('#').next().unwrap_or("").trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=CALLEPHIIN_CODENAME");
    let codename = std::env::var("CALLEPHIIN_CODENAME").unwrap_or_else(|_| codename_from_manifest());
    println!("cargo:rustc-env=CALLEPHIIN_CODENAME={codename}");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Embed the ico as the exe icon
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "callephiin");
        res.set("FileDescription", "callephiin");
        res.set("InternalName", "callephiin");
        res.set("OriginalFilename", "callephiin.exe");
        res.compile().expect("failed to embed Windows icon resource");
    }
}
