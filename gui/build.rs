fn main() {
    std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/main-window.slint", config).expect("Slint build failed");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/icons/windows/esvg.ico")
            .set("FileDescription", "ESVG")
            .set("ProductName", "ESVG")
            .set("InternalName", "esvg")
            .compile()
            .expect("Could not embed the Windows application icon");
    }
}
