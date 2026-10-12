use super::{
    bulk_drop_paths, create_export_output, decode_file_uri, export_extension, format_size,
    output_file_name, render_svg, REACT_EXPORT_FORMAT,
};
use crate::viewport::{frame_region, render_frame, ViewRequest};
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

fn view(zoom: f32, pan_x: f32, pan_y: f32, scale_factor: f32) -> ViewRequest {
    ViewRequest {
        zoom,
        pan_x,
        pan_y,
        viewport_width: 400.0,
        viewport_height: 300.0,
        scale_factor,
        show_optimized: true,
    }
}

#[test]
fn frame_region_covers_whole_svg_when_it_fits_and_is_pixel_aligned() {
    let region = frame_region(24.0, 12.0, &view(2.0, 10.25, 20.5, 1.5)).unwrap();
    assert_eq!(region.left, 15.0);
    assert_eq!(region.top, 30.0);
    // 24 * 2 * 1.5 = 72 physical px, plus one for the fractional start.
    assert_eq!((region.width, region.height), (73, 37));
}

#[test]
fn frame_region_is_limited_to_viewport_plus_overscan_when_zoomed_in() {
    // 2400x1200 logical px of SVG, viewport 400x300, overscan 100x75.
    let region = frame_region(24.0, 12.0, &view(100.0, -1000.0, -500.0, 1.0)).unwrap();
    assert_eq!((region.left, region.top), (-100.0, -75.0));
    assert_eq!((region.width, region.height), (600, 450));
}

#[test]
fn frame_region_is_none_when_svg_is_off_screen() {
    assert!(frame_region(24.0, 12.0, &view(1.0, 2000.0, 0.0, 1.0)).is_none());
    assert!(frame_region(24.0, 12.0, &view(0.0, 0.0, 0.0, 1.0)).is_none());
}

#[test]
fn viewport_frame_matches_a_full_render_cropped_to_the_same_area() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30" viewBox="0 0 40 30"><circle cx="20" cy="15" r="11.3" fill="#336699" fill-opacity="0.7"/><path d="M2 3 L37 27" stroke="#c03" stroke-width="0.6"/></svg>"##;
    let tree = usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default()).unwrap();
    // 10x zoom on a 1.5x display: 15 physical px per SVG unit, panned so the SVG is cropped.
    let request = view(10.0, -60.0, -40.0, 1.5);
    let frame = render_frame(&tree, &request).unwrap();
    let region = frame_region(40.0, 30.0, &request).unwrap();

    // The frame maps back to the viewport exactly where its pixels were rendered.
    assert!((request.pan_x + frame.x * request.zoom - region.left / 1.5).abs() < 1e-3);
    assert!((request.pan_y + frame.y * request.zoom - region.top / 1.5).abs() < 1e-3);

    let full = render_svg(&tree, 15.0);
    let (full_width, frame_width) = (full.width() as usize, frame.pixels.width() as usize);
    let origin_x = (-request.pan_x * 1.5 + region.left) as usize;
    let origin_y = (-request.pan_y * 1.5 + region.top) as usize;
    for y in 0..frame.pixels.height() as usize {
        let frame_row = &frame.pixels.as_slice()[y * frame_width..(y + 1) * frame_width];
        let start = (origin_y + y) * full_width + origin_x;
        assert_eq!(
            frame_row,
            &full.as_slice()[start..start + frame_width],
            "row {y}"
        );
    }
}
