pub mod arbiter;
pub mod helpers;
pub mod path;
pub mod plugin;
pub mod plugins;
pub mod svg;
pub mod transform;

pub use helpers::tree::{contains_element, has_markers};
use arbiter::Arbiter;
pub use plugin::{SingleElementPluginTrait, WholeSVGPluginTrait};
pub use plugins::{
    ApplyTransformsPlugin, CollapseGroupsPlugin, CombinePathsPlugin, CssToAttributesPlugin,
    MangleIdsPlugin, NumberPrecisionPlugin, OptimizeColorsPlugin, RemoveEmptyTextPlugin,
    RemoveUnnecessaryAttrsPlugin, RemoveUnnecessaryClipPathsPlugin, ShapeToPathPlugin,
    SimplifyPathsPlugin, SortAttrsPlugin,
};
pub use svg::Svg;

pub const PLUGIN_NAMES: [&str; 13] = [
    "Apply Transforms",
    "Collapse Groups",
    "Combine Paths",
    "CSS to Attributes",
    "Mangle IDs",
    "Number Precision",
    "Optimize Colors",
    "Remove Empty Text",
    "Remove Unnecessary Attrs",
    "Remove Unnecessary Clip Paths",
    "Shape to Path",
    "Simplify Paths",
    "Sort Attrs",
];

pub struct PluginConfig {
    pub apply_transforms: bool,
    pub collapse_groups: bool,
    pub combine_paths: bool,
    pub css_to_attributes: bool,
    pub mangle_ids: bool,
    pub number_precision: bool,
    pub optimize_colors: bool,
    pub remove_empty_text: bool,
    pub remove_unnecessary_attrs: bool,
    pub remove_unnecessary_clip_paths: bool,
    pub shape_to_path: bool,
    pub simplify_paths: bool,
    pub sort_attrs: bool,
}

impl Default for PluginConfig {
    fn default() -> Self {
        PluginConfig {
            apply_transforms: true,
            collapse_groups: true,
            combine_paths: true,
            css_to_attributes: true,
            mangle_ids: true,
            number_precision: true,
            optimize_colors: true,
            remove_empty_text: true,
            remove_unnecessary_attrs: true,
            remove_unnecessary_clip_paths: true,
            shape_to_path: true,
            simplify_paths: true,
            sort_attrs: true,
        }
    }
}

pub struct SvgExportOptions {
    pub include_viewbox: bool,
    pub include_dimensions: bool,
    pub width: f32,
    pub height: f32,
}

pub fn apply_svg_export_options(
    svg_string: &str,
    opts: &SvgExportOptions,
) -> Result<String, String> {
    let mut svg = Svg::from_string(&svg_string.to_string())?;
    let root = &mut svg.root;

    if !opts.include_viewbox {
        root.attributes.remove("viewBox");
    }

    if opts.include_dimensions {
        // Format as integers if they are whole numbers, otherwise as decimals
        let w_str = if opts.width.fract() == 0.0 {
            format!("{}", opts.width as i64)
        } else {
            format!("{:.2}", opts.width)
        };
        let h_str = if opts.height.fract() == 0.0 {
            format!("{}", opts.height as i64)
        } else {
            format!("{:.2}", opts.height)
        };
        root.attributes.insert("width".into(), w_str);
        root.attributes.insert("height".into(), h_str);
    } else {
        root.attributes.remove("width");
        root.attributes.remove("height");
    }

    Ok(svg.to_string())
}

/// Decimal places to keep, from the document's view size.
///
/// Coordinates are kept to about a thousandth of the view size, so small
/// viewBoxes such as `0 0 1 1` keep more digits than the default. Never fewer
/// than `default`, and never more than 8.
fn precision_for_view(root: &xmltree::Element, default: u32) -> u32 {
    let view_size = root
        .attributes
        .get("viewBox")
        .and_then(|v| helpers::number::parse_list(v))
        .filter(|v| v.len() == 4)
        .map(|v| v[2].abs().max(v[3].abs()))
        .or_else(|| {
            let w = root.attributes.get("width").and_then(|v| helpers::number::parse_plain(v));
            let h = root.attributes.get("height").and_then(|v| helpers::number::parse_plain(v));
            match (w, h) {
                (Some(w), Some(h)) => Some(w.max(h)),
                _ => None,
            }
        });

    match view_size {
        Some(size) if size > 0.0 && size.is_finite() => {
            let step = size * 1e-3;
            let needed = (-step.log10()).ceil();
            (needed.max(default as f64).min(8.0)) as u32
        }
        _ => default,
    }
}

pub fn optimize(svg_string: &str, config: &PluginConfig) -> Result<String, String> {
    let svg = Svg::from_string(&svg_string.to_string()).map_err(|e| e.to_string())?;
    let mut arbiter = Arbiter::new();

    // Single-element plugins
    if config.optimize_colors {
        arbiter.add_single_element_plugin(Box::new(OptimizeColorsPlugin));
    }
    // Converting a shape changes its element name, which stylesheet selectors match.
    if config.shape_to_path && !contains_element(&svg.root, "style") {
        arbiter.add_single_element_plugin(Box::new(ShapeToPathPlugin {
            convert_rects: !has_markers(&svg.root),
        }));
    }
    if config.simplify_paths {
        arbiter.add_single_element_plugin(Box::new(SimplifyPathsPlugin {
            keep_vertices: has_markers(&svg.root),
        }));
    }
    if config.sort_attrs {
        arbiter.add_single_element_plugin(Box::new(SortAttrsPlugin));
    }

    // Whole-SVG plugins
    if config.css_to_attributes {
        arbiter.add_whole_svg_plugin(Box::new(CssToAttributesPlugin));
    }
    if config.apply_transforms {
        arbiter.add_whole_svg_plugin(Box::new(ApplyTransformsPlugin));
    }
    if config.collapse_groups {
        arbiter.add_forced_whole_svg_plugin(Box::new(CollapseGroupsPlugin));
    }
    if config.remove_empty_text {
        arbiter.add_whole_svg_plugin(Box::new(RemoveEmptyTextPlugin));
    }
    if config.remove_unnecessary_clip_paths {
        arbiter.add_whole_svg_plugin(Box::new(RemoveUnnecessaryClipPathsPlugin));
    }
    if config.combine_paths {
        arbiter.add_whole_svg_plugin(Box::new(CombinePathsPlugin));
    }
    // After transforms are baked, so baked-in scales no longer magnify rounding.
    if config.number_precision {
        let precision = precision_for_view(&svg.root, 2);
        arbiter.add_whole_svg_plugin(Box::new(NumberPrecisionPlugin { precision }));
    }
    if config.remove_unnecessary_attrs {
        arbiter.add_whole_svg_plugin(Box::new(RemoveUnnecessaryAttrsPlugin));
    }
    if config.mangle_ids {
        arbiter.add_forced_whole_svg_plugin(Box::new(MangleIdsPlugin { prefix: None }));
    }

    let result = arbiter.process(&svg).map_err(|e| e.to_string())?;
    Ok(result.to_string())
}
