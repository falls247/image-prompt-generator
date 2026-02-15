fn main() {
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/app.ico");

    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_manifest_file("assets/app.manifest");
        res.set_icon("assets/app.ico");
        if let Err(err) = res.compile() {
            panic!("failed to embed Windows manifest: {err}");
        }
    }
}
