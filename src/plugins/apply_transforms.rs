// Plugin
//
// Name: Apply Transforms
// Author: Nikola Lazarov
//
// Description:
// Bakes transforms into geometry where that cannot change the rendering.
// - Leaf <path>, <rect> and <circle> elements whose transform is axis-aligned
//   (translate, scale, flips, matrix with zero b and c, and combinations of
//   these) get their coordinates rewritten and the transform removed.
// - Transforms on <g> and <a> are pushed down onto their children.
//
// Each transform list is composed into a single matrix first, so a list the
// plugin cannot read (rotate, skew, unitful values) is never partly applied.
//
// Nothing is baked where the transform's effect is visible through something
// the element does not own: clip paths, masks, filters, markers, url() paints
// (userSpaceOnUse gradients and patterns), stroke widths (scaling changes
// them), or any stylesheet or <use> element that could depend on the transform.
//
// Todo:
// [ ] - Bake rotations and skews (needs arc rotation and rounded-rect handling)

use crate::helpers::tree::{contains_element, has_markers, is_non_rendered, num_attr};
use crate::helpers::number::{format_number, parse_plain};
use crate::path::{Path, Segment};
use crate::plugin::WholeSVGPluginTrait;
use crate::transform::{parse_transform, Affine};
use crate::Svg;
use std::error::Error;
use xmltree::Element;

pub struct ApplyTransformsPlugin;

fn is_stroked(element: &Element) -> bool {
    element
        .attributes
        .get("stroke")
        .map_or(false, |v| v.trim() != "none")
        || element.attributes.get("style").map_or(false, |v| v.contains("stroke"))
}

/// True when the element has an attribute whose meaning depends on its own
/// coordinate system, so its geometry must not be rewritten.
fn is_pinned(element: &Element) -> bool {
    element.attributes.iter().any(|(name, value)| match name.as_str() {
        "transform-origin" | "clip-path" | "mask" | "filter" | "style" => true,
        "fill" | "stroke" => value.contains("url("),
        n => n.starts_with("marker"),
    })
}

/// Number of decimal digits kept when a value is written back is not relevant;
/// this only decides whether two scale factors are equal.
fn same_scale(a: f64, b: f64) -> bool {
    (a.abs() - b.abs()).abs() < 1e-12
}

fn is_identity_scale(m: &Affine) -> bool {
    (m.a - 1.0).abs() < 1e-12 && (m.d - 1.0).abs() < 1e-12
}

/// Applies an axis-aligned matrix to every point, and fixes the arc parameters.
fn transform_segments(segments: &[Segment], m: &Affine) -> Vec<Segment> {
    let flip = m.a * m.d < 0.0;
    segments
        .iter()
        .map(|seg| match *seg {
            Segment::Move { x, y } => {
                let (x, y) = m.apply(x, y);
                Segment::Move { x, y }
            }
            Segment::Line { x, y } => {
                let (x, y) = m.apply(x, y);
                Segment::Line { x, y }
            }
            Segment::Cubic { x1, y1, x2, y2, x, y } => {
                let (x1, y1) = m.apply(x1, y1);
                let (x2, y2) = m.apply(x2, y2);
                let (x, y) = m.apply(x, y);
                Segment::Cubic { x1, y1, x2, y2, x, y }
            }
            Segment::Quad { x1, y1, x, y } => {
                let (x1, y1) = m.apply(x1, y1);
                let (x, y) = m.apply(x, y);
                Segment::Quad { x1, y1, x, y }
            }
            Segment::Arc { rx, ry, rotation, large, sweep, x, y } => {
                let (x, y) = m.apply(x, y);
                // A mirror image turns the ellipse's rotation around and reverses
                // the direction it is traced in.
                let rotation = if flip { -rotation } else { rotation };
                let sweep = if flip { 1.0 - sweep } else { sweep };
                Segment::Arc {
                    rx: rx.abs() * m.a.abs(),
                    ry: ry.abs() * m.d.abs(),
                    rotation,
                    large,
                    sweep,
                    x,
                    y,
                }
            }
            Segment::Close => Segment::Close,
        })
        .collect()
}

/// Rewrites path data. Returns `false` when the path cannot be baked safely.
fn bake_path(element: &mut Element, m: &Affine) -> bool {
    let segments = match element
        .attributes
        .get("d")
        .and_then(|d| Path::parse(d))
        .and_then(|p| p.segments())
    {
        Some(s) => s,
        None => return false,
    };

    // An arc with a rotated ellipse is not preserved by non-uniform scaling.
    let uniform = same_scale(m.a, m.d);
    if !uniform {
        let rotated_arc = segments.iter().any(|s| match s {
            Segment::Arc { rotation, .. } => rotation.abs() > 1e-12,
            _ => false,
        });
        if rotated_arc {
            return false;
        }
    }

    let baked = transform_segments(&segments, m);
    element.attributes.insert("d".to_string(), Path::emit(&baked, None));
    true
}

fn bake_circle(element: &mut Element, m: &Affine) -> bool {
    let cx = match num_attr(element, "cx", 0.0) {
        Some(v) => v,
        None => return false,
    };
    let cy = match num_attr(element, "cy", 0.0) {
        Some(v) => v,
        None => return false,
    };
    let r = match element.attributes.get("r").and_then(|v| parse_plain(v)) {
        Some(v) if v >= 0.0 => v,
        _ => return false,
    };

    let (cx, cy) = m.apply(cx, cy);
    element.attributes.insert("cx".to_string(), format_number(cx));
    element.attributes.insert("cy".to_string(), format_number(cy));

    if same_scale(m.a, m.d) {
        element
            .attributes
            .insert("r".to_string(), format_number(r * m.a.abs()));
    } else {
        // A circle scaled differently on each axis is an ellipse.
        element.name = "ellipse".to_string();
        element.attributes.remove("r");
        element
            .attributes
            .insert("rx".to_string(), format_number(r * m.a.abs()));
        element
            .attributes
            .insert("ry".to_string(), format_number(r * m.d.abs()));
    }
    true
}

fn bake_rect(element: &mut Element, m: &Affine) -> bool {
    let x = match num_attr(element, "x", 0.0) {
        Some(v) => v,
        None => return false,
    };
    let y = match num_attr(element, "y", 0.0) {
        Some(v) => v,
        None => return false,
    };
    let (width, height) = match (
        element.attributes.get("width").and_then(|v| parse_plain(v)),
        element.attributes.get("height").and_then(|v| parse_plain(v)),
    ) {
        (Some(w), Some(h)) if w >= 0.0 && h >= 0.0 => (w, h),
        _ => return false,
    };

    // Rounded corners would be scaled by a different factor on each axis, so
    // they are only handled when the transform is a pure translation.
    let rounded = ["rx", "ry"].iter().any(|name| {
        element
            .attributes
            .get(*name)
            .map_or(false, |v| parse_plain(v) != Some(0.0))
    });
    if rounded && !is_identity_scale(m) {
        return false;
    }

    let (x0, y0) = m.apply(x, y);
    let (x1, y1) = m.apply(x + width, y + height);
    element
        .attributes
        .insert("x".to_string(), format_number(x0.min(x1)));
    element
        .attributes
        .insert("y".to_string(), format_number(y0.min(y1)));
    element
        .attributes
        .insert("width".to_string(), format_number((x1 - x0).abs()));
    element
        .attributes
        .insert("height".to_string(), format_number((y1 - y0).abs()));
    true
}

impl ApplyTransformsPlugin {
    /// Moves a group's transform onto each child, then removes it from the group.
    ///
    /// This keeps the rendering because a child's coordinate system is the group's
    /// system multiplied by the child's own transform. It does not hold for the
    /// group's own clip, mask or filter, which use the group's system, so those
    /// groups are skipped.
    fn push_down(element: &mut Element) {
        let group_transform = match element.attributes.get("transform") {
            Some(t) => t.clone(),
            None => return,
        };
        if is_pinned(element) || parse_transform(&group_transform).is_none() {
            return;
        }
        if !element.children.iter().any(|c| c.as_element().is_some()) {
            return;
        }

        for child in element.children.iter_mut() {
            if let Some(child_el) = child.as_mut_element() {
                // Clip paths, gradients, <defs> and the like are not drawn here.
                // They are used in the coordinate system of whatever references
                // them, so the group's transform never applied to them.
                if is_non_rendered(&child_el.name) {
                    continue;
                }
                let combined = match child_el.attributes.get("transform") {
                    Some(child_transform) => format!("{} {}", group_transform, child_transform),
                    None => group_transform.clone(),
                };
                child_el.attributes.insert("transform".to_string(), combined);
            }
        }

        element.attributes.remove("transform");
    }

    fn apply_recursive(element: &mut Element, scale_safe: bool, ancestor_stroked: bool) {
        if matches!(element.name.as_str(), "g" | "a") {
            Self::push_down(element);
        }

        let stroked = ancestor_stroked || is_stroked(element);

        if let Some(transform) = element.attributes.get("transform").cloned() {
            if !is_pinned(element) {
                if let Some(m) = parse_transform(&transform) {
                    if m.is_axis_aligned() {
                        let scaling = !is_identity_scale(&m);
                        // Scaling changes stroke widths and dash lengths, which
                        // the element may inherit from an ancestor or a <use>.
                        let allowed = !scaling || (scale_safe && !stroked);
                        if allowed {
                            let baked = match element.name.as_str() {
                                "path" => bake_path(element, &m),
                                "rect" => bake_rect(element, &m),
                                "circle" => bake_circle(element, &m),
                                _ => false,
                            };
                            if baked {
                                element.attributes.remove("transform");
                            }
                        }
                    }
                }
            }
        }

        for child in element.children.iter_mut() {
            if let Some(child_el) = child.as_mut_element() {
                Self::apply_recursive(child_el, scale_safe, stroked);
            }
        }
    }
}

impl WholeSVGPluginTrait for ApplyTransformsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();

        // A stylesheet can attach paint, clipping or filters to any element.
        if contains_element(&svg_clone.root, "style") {
            return Ok(svg_clone);
        }
        // A <use> can pass an inherited stroke width to what it references, and a
        // marker is sized by the coordinate system of the element it is drawn on.
        let scale_safe = !contains_element(&svg_clone.root, "use") && !has_markers(&svg_clone.root);
        // A stroke set on the root <svg> is inherited by everything.
        let root_stroked = is_stroked(&svg_clone.root);

        for child in svg_clone.root.children.iter_mut() {
            if let Some(child_el) = child.as_mut_element() {
                Self::apply_recursive(child_el, scale_safe, root_stroked);
            }
        }

        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        ApplyTransformsPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn bakes_translate_into_path() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path transform="translate(10 20)" d="M0 0 L5 5"/></svg>"##);
        assert!(!out.contains("transform"));
        assert!(out.contains("M10 20l5 5"), "{}", out);
    }

    #[test]
    fn keeps_rotation_untouched() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path transform="rotate(30) translate(5 0)" d="M0 0 L5 5"/></svg>"##);
        assert!(out.contains("rotate(30) translate(5 0)"));
    }

    #[test]
    fn mirror_flips_arc_sweep() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path transform="scale(-1 1)" d="M0 0 A10 10 0 0 1 20 0"/></svg>"##);
        assert!(!out.contains("transform"));
        // Sweep flag 1 becomes 0 after a mirror.
        assert!(out.contains("A10 10 0 00-20 0"), "{}", out);
    }

    #[test]
    fn rect_with_units_is_not_rewritten() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect transform="translate(5 5)" x="10%" width="5" height="5"/></svg>"##);
        assert!(out.contains("transform=\"translate(5 5)\""));
    }

    #[test]
    fn circle_with_units_does_not_panic() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><circle transform="translate(5 5)" cx="10px" r="5"/></svg>"##);
        assert!(out.contains("transform"));
    }

    #[test]
    fn rounded_rect_not_scaled() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect transform="scale(2)" width="10" height="10" rx="3"/></svg>"##);
        assert!(out.contains("transform=\"scale(2)\""));
    }

    #[test]
    fn scale_skipped_for_stroked_element() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path transform="scale(4)" stroke="#000" stroke-width="2" d="M0 0 L5 5"/></svg>"##);
        assert!(out.contains("transform=\"scale(4)\""));
    }

    #[test]
    fn clipped_element_not_baked() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path transform="translate(5 5)" clip-path="url(#c)" d="M0 0 L5 5"/></svg>"##);
        assert!(out.contains("translate(5 5)"));
    }

    #[test]
    fn group_transform_pushed_to_children() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(10 0)"><path transform="scale(2)" d="M0 0 L5 5"/></g></svg>"##);
        assert!(!out.contains("<g transform"));
        assert!(out.contains("M10 0L20 10"), "{}", out);
    }

    #[test]
    fn root_stroke_blocks_scale_baking() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg" stroke="#000"><path transform="scale(4)" d="M0 0 L5 5"/></svg>"##);
        assert!(out.contains("transform=\"scale(4)\""), "{}", out);
    }

    #[test]
    fn push_down_skips_clip_path_children() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(10 0)"><clipPath id="c"><rect width="5" height="5"/></clipPath><path d="M0 0 L5 5"/></g></svg>"##);
        assert!(out.contains("<clipPath id=\"c\"><rect"), "{}", out);
        assert!(!out.contains("<clipPath id=\"c\" transform"), "{}", out);
    }

    #[test]
    fn group_with_clip_keeps_transform() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><g transform="translate(10 0)" clip-path="url(#c)"><path d="M0 0 L5 5"/></g></svg>"##);
        assert!(out.contains("transform=\"translate(10 0)\""));
    }
}
