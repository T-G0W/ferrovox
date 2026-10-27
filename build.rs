fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Ferrovox")
            .set("FileDescription", "Ferrovox Soundboard – by T-GOW")
            .set("CompanyName", "T-GOW")
            .set("LegalCopyright", "© 2026 T-GOW");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Icon-Ressource konnte nicht eingebettet werden: {e}");
        }
    }
}
