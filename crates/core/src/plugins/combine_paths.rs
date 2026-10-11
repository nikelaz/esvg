use crate::helpers::bounding_box::{segments_bbox, BBox};
use crate::helpers::tree::{contains_element, has_markers};
use crate::path::{Path, Segment};
use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
/**
* Plugin
*
* Name: Combine Paths
* Author: Nikola Lazarov
*
* Description:
* Merges neighbouring <path> elements into one when that draws the same thing:
* - Their attributes are identical apart from `d`.
* - Nothing in the attributes depends on the individual path: no id, markers,
*   dashes, masks, filters, clipping, or partial transparency.
* - If the fill is painted, the paths' bounding boxes must not overlap. Under
*   nonzero or evenodd rules, overlapping subpaths would otherwise interact.
*   Strokes of an unpainted fill can overlap freely.
*
* Inherited properties count too: nothing is merged below an ancestor that sets
* partial transparency or dashes. Documents with a stylesheet, <use> or markers
* are left alone, since those can apply such properties invisibly.
*
* Paths with child elements (<title>, <animate>, ...) are never merged, because
* the children belong to one specific path.
*
* The merged data is written from absolute segments, so a relative leading
* moveto keeps its meaning. Whitespace between merged paths is dropped.
*/
use std::error::Error;
use xmltree::{Element, XMLNode};

pub struct CombinePathsPlugin;

/// True when an attribute value uses a colour with alpha.
fn has_partial_alpha(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    v.contains("rgba(") || v.contains("hsla(") || v == "transparent" || {
        v.starts_with('#') && (v.len() == 5 || v.len() == 9)
    }
}

/// True when merging this path into a neighbour could change the rendering.
fn depends_on_own_identity(element: &Element) -> bool {
    element.attributes.iter().any(|(name, value)| match name.as_str() {
        "id" | "pathLength" | "filter" | "mask" | "clip-path" | "style" => true,
        "stroke-dasharray" => value.trim() != "none",
        "opacity" | "fill-opacity" | "stroke-opacity" => value.trim() != "1",
        "fill" | "stroke" => has_partial_alpha(value),
        n => n.starts_with("marker"),
    })
}

/// True when an ancestor sets an inherited property that makes merging unsafe:
/// partial transparency or dashes apply per path, and an inline style is unknown.
fn passes_down_identity(element: &Element) -> bool {
    element.attributes.iter().any(|(name, value)| match name.as_str() {
        "style" => true,
        "stroke-dasharray" => value.trim() != "none",
        "fill-opacity" | "stroke-opacity" => value.trim() != "1",
        "fill" | "stroke" => has_partial_alpha(value),
        _ => false,
    })
}

/// True when the path has no content of its own besides whitespace.
fn has_no_children(element: &Element) -> bool {
    element.children.iter().all(is_whitespace_text)
}

fn fill_is_painted(element: &Element) -> bool {
    element
        .attributes
        .get("fill")
        .map_or(true, |v| v.trim() != "none")
}

fn path_segments(element: &Element) -> Option<Vec<Segment>> {
    let segments = Path::parse(element.attributes.get("d")?)?.segments()?;
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

fn same_attributes_except_d(a: &Element, b: &Element) -> bool {
    let without_d = |e: &Element| e.attributes.len() - usize::from(e.attributes.contains_key("d"));
    without_d(a) == without_d(b)
        && a.attributes
            .iter()
            .filter(|(name, _)| name.as_str() != "d")
            .all(|(name, value)| b.attributes.get(name) == Some(value))
}

fn is_whitespace_text(node: &XMLNode) -> bool {
    matches!(node, XMLNode::Text(t) if t.trim().is_empty())
}

impl CombinePathsPlugin {
    fn traverse_paths(element: &mut Element, ancestor_unsafe: bool) {
        let unsafe_here = ancestor_unsafe || passes_down_identity(element);
        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::traverse_paths(child_element, unsafe_here);
            }
        }

        if !unsafe_here {
            Self::combine_adjacent_paths(element);
        }
    }

    fn combine_adjacent_paths(element: &mut Element) {
        let children = std::mem::take(&mut element.children);
        let mut out: Vec<XMLNode> = Vec::with_capacity(children.len());
        let mut i = 0;

        while i < children.len() {
            let first = match children[i].as_element() {
                Some(e) if e.name == "path" && !depends_on_own_identity(e) && has_no_children(e) => e,
                _ => {
                    out.push(children[i].clone());
                    i += 1;
                    continue;
                }
            };
            let first_segments = match path_segments(first) {
                Some(s) => s,
                None => {
                    out.push(children[i].clone());
                    i += 1;
                    continue;
                }
            };

            let painted = fill_is_painted(first);
            let mut boxes: Vec<BBox> = Vec::new();
            if painted {
                match segments_bbox(&first_segments) {
                    Some(b) => boxes.push(b),
                    None => {
                        out.push(children[i].clone());
                        i += 1;
                        continue;
                    }
                }
            }

            let mut run_segments = first_segments;
            let mut last = i;
            let mut j = i + 1;
            while j < children.len() {
                if is_whitespace_text(&children[j]) {
                    j += 1;
                    continue;
                }
                let next = match children[j].as_element() {
                    Some(e)
                        if e.name == "path"
                            && !depends_on_own_identity(e)
                            && has_no_children(e)
                            && same_attributes_except_d(first, e) =>
                    {
                        e
                    }
                    _ => break,
                };
                let next_segments = match path_segments(next) {
                    Some(s) => s,
                    None => break,
                };
                if painted {
                    let next_box = match segments_bbox(&next_segments) {
                        Some(b) => b,
                        None => break,
                    };
                    if boxes.iter().any(|b| b.overlaps(&next_box)) {
                        break;
                    }
                    boxes.push(next_box);
                }
                run_segments.extend(next_segments);
                last = j;
                j += 1;
            }

            if last == i {
                out.push(children[i].clone());
                i += 1;
                continue;
            }

            let mut combined = first.clone();
            combined
                .attributes
                .insert("d".to_string(), Path::emit(&run_segments, None));
            out.push(XMLNode::Element(combined));
            i = last + 1;
        }

        element.children = out;
    }
}

impl WholeSVGPluginTrait for CombinePathsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        let root = &svg_clone.root;
        if contains_element(root, "style") || contains_element(root, "use") || has_markers(root) {
            return Ok(svg_clone);
        }
        Self::traverse_paths(&mut svg_clone.root, false);

        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        CombinePathsPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn merges_disjoint_paths() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#000" d="M0 0h10v10H0z"/><path fill="#000" d="M50 0h10v10H50z"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 1, "{}", out);
    }

    #[test]
    fn checks_overlap_against_every_path_in_run() {
        // The third path overlaps the second but not the first.
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#000" fill-rule="evenodd" d="M10 10H40V40H10Z"/><path fill="#000" fill-rule="evenodd" d="M60 10H120V70H60Z"/><path fill="#000" fill-rule="evenodd" d="M100 40H170V120H100Z"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 2, "{}", out);
    }

    #[test]
    fn does_not_merge_across_curve_bulge() {
        // The curve's control points reach into the first rectangle.
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#c60" d="M80 100H120V150H80Z"/><path fill="#c60" d="M20 -40C120 160 120 160 20 40Z"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 2, "{}", out);
    }

    #[test]
    fn does_not_merge_partial_alpha_or_dashes() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#000" opacity="0.5" d="M0 0h1v1H0z"/><path fill="#000" opacity="0.5" d="M5 0h1v1H5z"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 2);
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="none" stroke="#000" stroke-dasharray="2 2" d="M0 0h1"/><path fill="none" stroke="#000" stroke-dasharray="2 2" d="M5 0h1"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 2);
    }

    #[test]
    fn does_not_merge_under_inherited_alpha() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><g stroke="#000" stroke-opacity="0.5"><path fill="none" d="M0 0L10 10"/><path fill="none" d="M0 10L10 0"/></g></svg>"##);
        assert_eq!(out.matches("<path").count(), 2, "{}", out);
    }

    #[test]
    fn does_not_merge_paths_with_children() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#000" d="M0 0h1v1H0z"/><path fill="#000" d="M5 0h1v1H5z"><title>x</title></path></svg>"##);
        assert_eq!(out.matches("<path").count(), 2, "{}", out);
    }

    #[test]
    fn keeps_relative_leading_moveto_relative_to_origin() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="none" stroke="#000" d="M0 0 L10 0"/><path fill="none" stroke="#000" d="m20 20 l5 5"/></svg>"##);
        assert_eq!(out.matches("<path").count(), 1, "{}", out);
        assert!(out.contains("M20 20"), "{}", out);
    }
}
