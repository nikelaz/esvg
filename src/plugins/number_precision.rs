// Plugin
//
// Name: Number Precision
// Author: Nikola Lazarov
//
// Description:
// Rounds numeric values to a limited number of decimal places. `precision` is
// the number of decimals to keep at the root of the document; the library picks
// it from the view size so that small viewBoxes (for example `0 0 1 1`) keep
// enough digits.
//
// Coordinates under a scaling transform are magnified when drawn, so their
// rounding error is too. Each element therefore keeps extra decimals for the
// scale accumulated from its own and its ancestors' transforms.
//
// Content whose drawing scale cannot be known from the tree is not rounded:
// marker, pattern, symbol and filter contents, nested <svg>, clip paths and
// masks in bounding-box units, gradients, and anything a <use> references.
//
// Path data is rounded on an absolute grid: relative offsets are derived from
// the rounded positions, so rounding error does not accumulate along the path.
// Values that are not plain numbers (`10px`, `50%`, lists in `x`/`y`) are left
// unchanged.

use crate::helpers::number::{format_number, parse_list, parse_plain};
use crate::path::Path;
use crate::plugin::WholeSVGPluginTrait;
use crate::transform::parse_transform;
use crate::Svg;
use std::collections::HashSet;
use std::error::Error;
use xmltree::Element;

const NUMERIC_ATTRIBUTES: &[&str] = &[
    "x",
    "y",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "x1",
    "y1",
    "x2",
    "y2",
    "dx",
    "dy",
    "opacity",
    "fill-opacity",
    "stroke-opacity",
    "stroke-width",
    "stroke-miterlimit",
    "stroke-dashoffset",
    "font-size",
    "letter-spacing",
    "word-spacing",
    "baseline-shift",
    "offset",
];

/// Most decimals ever kept. Beyond this, f32 output cannot represent the value
/// more precisely anyway.
const MAX_DECIMALS: u32 = 10;

pub struct NumberPrecisionPlugin {
    /// Number of decimal places to keep at the root of the document.
    pub precision: u32,
}

/// True when an element sets up a coordinate system the plugin cannot measure,
/// so neither it nor its contents are rounded.
fn has_unknown_scale(element: &Element, is_root: bool) -> bool {
    let bbox_units = |attr: &str| element.attributes.get(attr).map(String::as_str) == Some("objectBoundingBox");
    match element.name.as_str() {
        "marker" | "pattern" | "symbol" | "filter" | "linearGradient" | "radialGradient" => true,
        "svg" => !is_root,
        "clipPath" => bbox_units("clipPathUnits"),
        "mask" => bbox_units("maskContentUnits"),
        _ => false,
    }
}

/// How much a transform enlarges distances: the longer of its two column vectors.
fn transform_scale(value: &str) -> Option<f64> {
    let m = parse_transform(value)?;
    Some((m.a * m.a + m.b * m.b).sqrt().max((m.c * m.c + m.d * m.d).sqrt()))
}

/// Ids that a <use> draws, so their subtrees are drawn at the <use>'s scale.
fn collect_use_targets(element: &Element, out: &mut HashSet<String>) {
    if element.name == "use" {
        for key in ["href", "xlink:href"] {
            if let Some(id) = element.attributes.get(key).and_then(|v| v.strip_prefix('#')) {
                out.insert(id.to_string());
            }
        }
    }
    for child in element.children.iter().filter_map(|c| c.as_element()) {
        collect_use_targets(child, out);
    }
}

impl NumberPrecisionPlugin {
    fn round(value: f64, decimals: u32) -> f64 {
        let factor = 10f64.powi(decimals as i32);
        (value * factor).round() / factor
    }

    fn format(value: f64, decimals: u32) -> String {
        format_number(Self::round(value, decimals))
    }

    fn round_list(value: &str, decimals: u32) -> Option<String> {
        let values = parse_list(value)?;
        Some(
            values
                .iter()
                .map(|v| Self::format(*v, decimals))
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    fn round_path(d: &str, decimals: u32) -> Option<String> {
        let segments = Path::parse(d)?.segments()?;
        Some(Path::emit(&segments, Some(decimals)))
    }

    /// Rounds one element's own numeric attributes to `decimals` places.
    fn round_element(element: &mut Element, decimals: u32) {
        if let Some(d) = element.attributes.get("d").cloned() {
            if let Some(rounded) = Self::round_path(&d, decimals) {
                element.attributes.insert("d".to_string(), rounded);
            }
        }

        for name in NUMERIC_ATTRIBUTES {
            if let Some(value) = element.attributes.get(*name) {
                if let Some(v) = parse_plain(value) {
                    let rounded = Self::format(v, decimals);
                    element.attributes.insert(name.to_string(), rounded);
                }
            }
        }

        for name in ["viewBox", "stroke-dasharray"] {
            if let Some(value) = element.attributes.get(name).cloned() {
                if let Some(rounded) = Self::round_list(&value, decimals) {
                    element.attributes.insert(name.to_string(), rounded);
                }
            }
        }
    }

    /// Decimals needed so that error magnified by `scale` stays at the root precision.
    fn decimals_for_scale(&self, scale: f64) -> u32 {
        let extra = if scale > 1.0 { scale.log10().ceil() as u32 } else { 0 };
        (self.precision + extra).min(MAX_DECIMALS)
    }

    fn walk(&self, element: &mut Element, scale: f64, is_root: bool, use_targets: &HashSet<String>) {
        if has_unknown_scale(element, is_root) {
            return;
        }
        if element
            .attributes
            .get("id")
            .map_or(false, |id| use_targets.contains(id))
        {
            return;
        }

        // The element's own transform applies to its own coordinates too.
        let local_scale = match element.attributes.get("transform") {
            Some(value) => match transform_scale(value) {
                Some(s) => scale * s,
                None => return,
            },
            None => scale,
        };

        Self::round_element(element, self.decimals_for_scale(local_scale));

        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                self.walk(child_element, local_scale, false, use_targets);
            }
        }
    }
}

impl WholeSVGPluginTrait for NumberPrecisionPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        let mut use_targets = HashSet::new();
        collect_use_targets(&svg_clone.root, &mut use_targets);
        self.walk(&mut svg_clone.root, 1.0, true, &use_targets);
        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(precision: u32, input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        NumberPrecisionPlugin { precision }
            .process(&mut svg)
            .unwrap()
            .to_string()
    }

    #[test]
    fn rounds_attributes() {
        let out = run(2, r##"<svg xmlns="http://www.w3.org/2000/svg"><rect x="10.12345" y="10.10" width="10px"/></svg>"##);
        assert!(out.contains("x=\"10.12\""), "{}", out);
        assert!(out.contains("y=\"10.1\""), "{}", out);
        assert!(out.contains("width=\"10px\""), "{}", out);
    }

    #[test]
    fn rounds_path_on_grid_without_drift() {
        let d = format!("M0 0{}", " h 0.1234".repeat(100));
        let rounded = NumberPrecisionPlugin::round_path(&d, 2).unwrap();
        let segs = Path::parse(&rounded).unwrap().segments().unwrap();
        let last = match segs.last().unwrap() {
            crate::path::Segment::Line { x, .. } => *x,
            other => panic!("unexpected {:?}", other),
        };
        // 100 steps of 0.1234 reach 12.34. Rounding each step to 0.12 would drift
        // to 12.0; the absolute-grid emitter lands on the correct 12.34.
        assert!((last - 12.34).abs() < 1e-6, "got {}", last);
    }

    #[test]
    fn leaves_invalid_path_alone() {
        let out = run(2, r##"<svg xmlns="http://www.w3.org/2000/svg"><path d="M1.2345"/></svg>"##);
        assert!(out.contains("d=\"M1.2345\""), "{}", out);
    }

    #[test]
    fn keeps_more_decimals_under_scale() {
        let out = run(2, r##"<svg xmlns="http://www.w3.org/2000/svg"><g transform="scale(100)"><path d="M0.12345 0.5L1 1"/></g></svg>"##);
        assert!(out.contains("M0.1235"), "{}", out);
    }

    #[test]
    fn skips_content_with_unknown_scale() {
        let out = run(2, r##"<svg xmlns="http://www.w3.org/2000/svg"><symbol id="s"><path d="M0.12345 0L1 1"/></symbol><defs><path id="p" d="M0.12345 0L1 1"/></defs><use href="#p"/></svg>"##);
        assert_eq!(out.matches("M0.12345").count(), 2, "{}", out);
    }
}
