use crate::helpers::tree::contains_element;
use crate::helpers::bounding_box::{compute_element_bbox, BBox};
use crate::helpers::number::parse_plain;
use crate::path::{Path, Segment};
use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
/**
 * Plugin
 *
 * Name: Remove Unnecessary Clip Paths
 * Author: Nikola Lazarov
 *
 * Description:
 * Removes clip-path references and definitions that have no visual effect.
 * - A `clip-path` is removed from an element when the clip is a single plain
 *   rectangle (userSpaceOnUse, no transform, no rounded corners) that fully
 *   contains the element's geometry. Elements that paint outside their geometry
 *   (strokes, filters, markers, including inherited ones) are never changed.
 * - A clipPath that nothing references is removed.
 *
 * Empty clip paths are kept: a reference to one hides the element, so removing
 * the reference would make it visible.
 *
 * Documents that use <style> or <use> are left alone, because their rules and
 * references can apply paint that the plugin cannot see.
 */
use std::collections::{HashMap, HashSet};
use std::error::Error;
use xmltree::{Element, XMLNode};

pub struct RemoveUnnecessaryClipPathsPlugin;

/// Returns the id from a `url(#id)` value.
fn parse_clip_path_url(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("url(")?.strip_suffix(')')?;
    let inner = inner.trim().trim_matches(|c| c == '\'' || c == '"').trim();
    inner.strip_prefix('#')
}

/// Collects every id referenced as `url(#id)` in `text`.
fn collect_urls(text: &str, out: &mut HashSet<String>) {
    let mut rest = text;
    while let Some(pos) = rest.find("url(") {
        rest = &rest[pos + 4..];
        let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '\'' || c == '"');
        if let Some(after_hash) = trimmed.strip_prefix('#') {
            let end = after_hash
                .find(|c: char| c == ')' || c == '\'' || c == '"' || c.is_whitespace())
                .unwrap_or(after_hash.len());
            out.insert(after_hash[..end].to_string());
        }
    }
}

/// Collects every `url(#id)` reference in attributes and <style> text.
fn collect_references(element: &Element, out: &mut HashSet<String>) {
    for value in element.attributes.values() {
        collect_urls(value, out);
    }
    if element.name == "style" {
        for node in &element.children {
            match node {
                XMLNode::Text(text) | XMLNode::CData(text) => collect_urls(text, out),
                _ => {}
            }
        }
    }
    for child in element.children.iter().filter_map(|c| c.as_element()) {
        collect_references(child, out);
    }
}

/// True when the element's own attributes can paint outside its geometry.
fn own_paint_risk(element: &Element) -> bool {
    element.attributes.iter().any(|(name, value)| match name.as_str() {
        "stroke" => value.trim() != "none",
        "filter" => true,
        "style" => ["stroke", "filter", "marker"].iter().any(|p| value.contains(p)),
        n => n.starts_with("marker"),
    })
}

/// True when the element or anything below it can paint outside its geometry.
fn subtree_paint_risk(element: &Element) -> bool {
    own_paint_risk(element)
        || element
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .any(subtree_paint_risk)
}

/// Attributes that can hide a clip shape. A hidden shape clips everything away.
const HIDING_ATTRIBUTES: &[&str] = &["visibility", "display", "style"];

/// The box of a closed axis-aligned rectangle drawn as a path, such as the
/// `M0 0H10V10H0Z` that Shape to Path produces for a <rect>.
fn rect_from_path(element: &Element) -> Option<BBox> {
    let segments = Path::parse(element.attributes.get("d")?)?.segments()?;
    let mut points: Vec<(f64, f64)> = Vec::new();
    let last = segments.len().checked_sub(1)?;
    for (index, seg) in segments.iter().enumerate() {
        match *seg {
            Segment::Move { x, y } if index == 0 => points.push((x, y)),
            Segment::Line { x, y } if index > 0 => points.push((x, y)),
            Segment::Close if index == last => {}
            _ => return None,
        }
    }
    if points.len() == 5 && points[4] == points[0] {
        points.pop();
    }
    if points.len() != 4 {
        return None;
    }
    // Every edge, including the closing one, must be horizontal or vertical,
    // and the four points must be the four distinct corners of their box.
    for i in 0..4 {
        let (a, b) = (points[i], points[(i + 1) % 4]);
        if a.0 != b.0 && a.1 != b.1 {
            return None;
        }
    }
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let corners = [(min_x, min_y), (max_x, min_y), (max_x, max_y), (min_x, max_y)];
    if !corners.iter().all(|c| points.contains(c)) {
        return None;
    }
    Some(BBox::new(min_x, min_y, max_x, max_y))
}

/// The box of a clip path when it is a single plain rectangle, otherwise `None`.
fn clip_box(clip: &Element) -> Option<BBox> {
    if clip
        .attributes
        .get("clipPathUnits")
        .map_or(false, |v| v != "userSpaceOnUse")
    {
        return None;
    }
    if clip.attributes.contains_key("transform")
        || clip.attributes.contains_key("clip-path")
        || HIDING_ATTRIBUTES.iter().any(|a| clip.attributes.contains_key(*a))
    {
        return None;
    }

    let mut children = clip.children.iter().filter_map(|c| c.as_element());
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    if child.attributes.contains_key("transform")
        || child.attributes.contains_key("clip-path")
        || child.attributes.contains_key("mask")
        || HIDING_ATTRIBUTES.iter().any(|a| child.attributes.contains_key(*a))
    {
        return None;
    }
    match child.name.as_str() {
        "rect" => {
            for corner in ["rx", "ry"] {
                if let Some(value) = child.attributes.get(corner) {
                    if parse_plain(value) != Some(0.0) {
                        return None;
                    }
                }
            }
            compute_element_bbox(child)
        }
        "path" => rect_from_path(child),
        _ => None,
    }
}

impl RemoveUnnecessaryClipPathsPlugin {
    /// Removes `clip-path` from elements that the clip cannot affect.
    fn remove_redundant_references(
        element: &mut Element,
        clip_boxes: &HashMap<String, BBox>,
        ancestor_risk: bool,
    ) {
        let risky = ancestor_risk || subtree_paint_risk(element);

        if !risky {
            let reference = element
                .attributes
                .get("clip-path")
                .and_then(|v| parse_clip_path_url(v))
                .and_then(|id| clip_boxes.get(id).copied());

            if let Some(clip) = reference {
                if let Some(geometry) = compute_element_bbox(element) {
                    if clip.contains(&geometry) {
                        element.attributes.remove("clip-path");
                    }
                }
            }
        }

        let child_risk = ancestor_risk || own_paint_risk(element);
        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::remove_redundant_references(child_element, clip_boxes, child_risk);
            }
        }
    }
}

impl WholeSVGPluginTrait for RemoveUnnecessaryClipPathsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        let root = &mut svg_clone.root;

        // Clip paths defined at the top level, with their containment box when usable.
        let mut clip_boxes: HashMap<String, BBox> = HashMap::new();
        let mut defined: Vec<String> = Vec::new();
        for defs in root.children.iter().filter_map(|c| c.as_element()) {
            if defs.name != "defs" {
                continue;
            }
            for clip in defs.children.iter().filter_map(|c| c.as_element()) {
                if clip.name != "clipPath" {
                    continue;
                }
                if let Some(id) = clip.attributes.get("id") {
                    defined.push(id.clone());
                    if let Some(bbox) = clip_box(clip) {
                        clip_boxes.insert(id.clone(), bbox);
                    }
                }
            }
        }

        // <style> rules and <use> references can apply paint the plugin cannot see.
        if !clip_boxes.is_empty() && !contains_element(root, "style") && !contains_element(root, "use") {
            Self::remove_redundant_references(root, &clip_boxes, false);
        }

        // Remove clip paths that nothing references any more.
        let mut referenced: HashSet<String> = HashSet::new();
        collect_references(root, &mut referenced);
        let orphaned: HashSet<String> = defined
            .into_iter()
            .filter(|id| !referenced.contains(id))
            .collect();

        if !orphaned.is_empty() {
            for defs in root.children.iter_mut().filter_map(|c| c.as_mut_element()) {
                if defs.name != "defs" {
                    continue;
                }
                defs.children.retain(|node| match node.as_element() {
                    Some(el) if el.name == "clipPath" => el
                        .attributes
                        .get("id")
                        .map_or(true, |id| !orphaned.contains(id)),
                    _ => true,
                });
            }
        }

        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Svg;

    fn optimize(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        RemoveUnnecessaryClipPathsPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn keeps_reference_to_empty_clip_path() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="e"/></defs><rect x="1" y="1" width="5" height="5" clip-path="url(#e)"/></svg>"##,
        );
        assert!(out.contains("clip-path=\"url(#e)\""));
    }

    #[test]
    fn removes_reference_when_rect_clip_contains_element() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><rect x="0" y="0" width="100" height="100"/></clipPath></defs><rect x="10" y="10" width="20" height="20" clip-path="url(#c)"/></svg>"##,
        );
        assert!(!out.contains("clip-path"));
        assert!(!out.contains("clipPath"), "orphaned clip removed");
    }

    #[test]
    fn rectangle_path_clip_is_recognised() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><path d="M0 0H100V100H0Z"/></clipPath></defs><rect x="10" y="10" width="20" height="20" clip-path="url(#c)"/></svg>"##,
        );
        assert!(!out.contains("clip-path"), "{}", out);
    }

    #[test]
    fn hidden_clip_shape_is_kept() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><rect width="100" height="100" visibility="hidden"/></clipPath></defs><rect x="10" y="10" width="20" height="20" clip-path="url(#c)"/></svg>"##,
        );
        assert!(out.contains("clip-path=\"url(#c)\""), "{}", out);
    }

    #[test]
    fn non_rectangular_path_clip_is_kept() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><path d="M0 0H100V100Z"/></clipPath></defs><rect x="10" y="10" width="20" height="20" clip-path="url(#c)"/></svg>"##,
        );
        assert!(out.contains("clip-path=\"url(#c)\""), "{}", out);
    }

    #[test]
    fn keeps_reference_when_element_pokes_out() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><rect x="0" y="0" width="100" height="100"/></clipPath></defs><rect x="50" y="50" width="100" height="20" clip-path="url(#c)"/></svg>"##,
        );
        assert!(out.contains("clip-path=\"url(#c)\""));
    }

    #[test]
    fn keeps_reference_for_stroked_element() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><rect x="0" y="0" width="100" height="100"/></clipPath></defs><rect x="10" y="10" width="20" height="20" stroke="#000" stroke-width="8" clip-path="url(#c)"/></svg>"##,
        );
        assert!(out.contains("clip-path=\"url(#c)\""));
    }

    #[test]
    fn keeps_clip_referenced_from_style() {
        let out = optimize(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><clipPath id="c"><rect width="1" height="1"/></clipPath></defs><style>.a{clip-path:url(#c)}</style><rect class="a" width="5" height="5"/></svg>"##,
        );
        assert!(out.contains("clipPath"));
    }
}
