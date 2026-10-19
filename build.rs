fn main() {
    slint_build::compile("ui/main.slint").expect("Slint UI compilation failed");

    // build.rs runs on the host, so #[cfg(windows)] here reflects the host OS,
    // not the build target -- it would silently skip the icon on a Linux ->
    // Windows cross build. CARGO_CFG_TARGET_OS is set by cargo for the target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=ui/icons/resource.rc");
        println!("cargo:rerun-if-changed=ui/icons/QAC_Logo.ico");
        embed_resource::compile("ui/icons/resource.rc", &[] as &[&str])
            .manifest_optional()
            .expect("Failed to embed Windows resources");
    }
}
