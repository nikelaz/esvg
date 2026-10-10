fn main() {
    std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1");
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/main-window.slint", config).expect("Slint build failed");
}
