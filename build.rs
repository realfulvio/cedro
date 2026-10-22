// Su Windows incorpora l'icona e i dati di versione nell'exe.
fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/cedro.ico");
        res.set("FileDescription", "Cedro");
        res.set("ProductName", "Cedro");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icona non incorporata: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/cedro.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
