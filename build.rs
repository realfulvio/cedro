// Su Windows incorpora l'icona e i dati di versione nell'exe.
fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/cedro.ico");
        res.set("FileDescription", "Cedro");
        res.set("ProductName", "Cedro");
        // Controlli con lo stile attuale di Windows nella finestra delle impostazioni.
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
<compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application></compatibility>
</assembly>"#,
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=icona non incorporata: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/cedro.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
