use super::{
    bulk_drop_paths, create_export_output, decode_file_uri, export_extension, format_size,
    output_file_name, REACT_EXPORT_FORMAT,
};
use esvg_rs::{optimize, PluginConfig, SvgExportOptions};
use resvg::usvg;
use std::path::Path;

const SVG_FIXTURE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12" viewBox="0 0 24 12"><rect width="24" height="12" fill="#336699"/></svg>"##;

fn export_options() -> SvgExportOptions {
    SvgExportOptions {
        include_viewbox: true,
        include_dimensions: false,
        width: 24.0,
        height: 12.0,
    }
}

fn decoded_test_path(unix_path: &str, windows_path: &str) -> std::path::PathBuf {
    if cfg!(windows) {
        windows_path.into()
    } else {
        unix_path.into()
    }
}

#[test]
fn formats_file_sizes_at_byte_kilobyte_and_megabyte_boundaries() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1024 * 1024), "1.0 MB");
}

#[test]
fn decodes_local_file_uris_and_rejects_non_file_or_malformed_uris() {
    assert_eq!(
        decode_file_uri("file:///tmp/my%20icon.svg"),
        Some(decoded_test_path("/tmp/my icon.svg", "tmp/my icon.svg"))
    );
    assert_eq!(
        decode_file_uri("file://localhost/tmp/icon.svg"),
        Some(decoded_test_path("/tmp/icon.svg", "tmp/icon.svg"))
    );
    assert_eq!(decode_file_uri("https://example.com/icon.svg"), None);
    assert_eq!(decode_file_uri("file:///tmp/%FF.svg"), None);
    assert_eq!(decode_file_uri("#fragment"), None);
}

#[test]
fn parses_multiple_dropped_file_uris_and_skips_other_lines() {
    assert_eq!(
        bulk_drop_paths("file:///tmp/a.svg\ntext/plain\nfile:///tmp/b%20icon.svg"),
        vec![
            decoded_test_path("/tmp/a.svg", "tmp/a.svg"),
            decoded_test_path("/tmp/b icon.svg", "tmp/b icon.svg")
        ]
    );
}

#[test]
fn format_selection_uses_the_expected_output_extension() {
    assert_eq!(export_extension("SVG"), "svg");
    assert_eq!(export_extension("PNG"), "png");
    assert_eq!(export_extension(REACT_EXPORT_FORMAT), "jsx");
    assert_eq!(export_extension("unknown"), "svg");
    assert_eq!(
        output_file_name(
            Path::new("logo.svg"),
            "-component.jsx",
            export_extension(REACT_EXPORT_FORMAT)
        ),
        "logo-component.jsx"
    );
}

#[test]
fn end_to_end_svg_optimization_exports_valid_svg_react_and_png() {
    let optimized = optimize(SVG_FIXTURE, &PluginConfig::default()).unwrap();
    assert!(usvg::Tree::from_data(optimized.as_bytes(), &usvg::Options::default()).is_ok());

    let svg =
        create_export_output(&optimized, "SVG", "blue_icon", &export_options(), 0.0, 0.0).unwrap();
    let svg = String::from_utf8(svg).unwrap();
    let svg_doc = roxmltree::Document::parse(&svg).unwrap();
    let root = svg_doc.root_element();
    assert_eq!(root.tag_name().name(), "svg");
    assert!(root.attribute("viewBox").is_some());
    assert!(usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default()).is_ok());

    let react = create_export_output(
        &optimized,
        REACT_EXPORT_FORMAT,
        "blue_icon",
        &export_options(),
        0.0,
        0.0,
    )
    .unwrap();
    let react = String::from_utf8(react).unwrap();
    assert!(react.contains("export default function BlueIcon(props)"));
    assert!(react.contains("<svg "));

    let png = create_export_output(
        &optimized,
        "PNG",
        "blue_icon",
        &export_options(),
        32.0,
        16.0,
    )
    .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 32);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 16);
}

#[test]
fn svg_export_applies_viewbox_and_dimension_preferences() {
    let optimized = optimize(SVG_FIXTURE, &PluginConfig::default()).unwrap();
    let options = SvgExportOptions {
        include_viewbox: false,
        include_dimensions: true,
        width: 48.0,
        height: 24.0,
    };
    let output = create_export_output(&optimized, "SVG", "icon", &options, 0.0, 0.0).unwrap();
    let document = roxmltree::Document::parse(std::str::from_utf8(&output).unwrap()).unwrap();
    let root = document.root_element();
    assert!(root.attribute("viewBox").is_none());
    assert_eq!(root.attribute("width"), Some("48"));
    assert_eq!(root.attribute("height"), Some("24"));
}

#[test]
fn export_pipeline_returns_errors_for_invalid_png_and_react_inputs() {
    assert!(create_export_output("<svg>", "PNG", "broken", &export_options(), 0.0, 0.0,).is_err());
    assert!(create_export_output(
        "<html/>",
        REACT_EXPORT_FORMAT,
        "broken",
        &export_options(),
        0.0,
        0.0,
    )
    .is_err());
}
