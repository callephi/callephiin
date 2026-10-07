fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Embeds ico for Windows
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
