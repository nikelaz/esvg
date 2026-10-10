// Plugin
//
// Name: Shape to Path
// Author: Nikola Lazarov
//
// Description:
// Converts <rect>, <line>, <polyline> and <polygon> to <path>.
// <circle> and <ellipse> are not converted, since the element form is shorter.
//
// Shapes are only converted when every value is a plain number. A value with a
// unit or percentage would not map to path data, so that shape is left alone.
// Markers do not apply to <rect> but would apply to the resulting <path>, and
// marker properties are inherited, so rects are only converted when the
// document cannot draw markers (`convert_rects`). A rect with an inline style
// is also left alone.

use crate::helpers::number::{parse_list, parse_plain};
use crate::path::{Path, Segment};
use crate::plugin::SingleElementPluginTrait;
use std::error::Error;
use xmltree::Element;

pub struct ShapeToPathPlugin {
    /// Convert <rect>. Turn off when the document has markers.
    pub convert_rects: bool,
}

impl ShapeToPathPlugin {
    /// Plain number attribute with a default for when it is missing.
    fn number(element: &Element, name: &str, default: f64) -> Option<f64> {
        match element.attributes.get(name) {
            None => Some(default),
            Some(value) => parse_plain(value),
        }
    }

    fn set_path(element: &mut Element, segments: &[Segment], remove: &[&str]) {
        element.name = "path".to_string();
        for name in remove {
            element.attributes.remove(*name);
        }
        element
            .attributes
            .insert("d".to_string(), Path::emit(segments, None));
    }

    fn rect_to_path(element: &mut Element) -> bool {
        let x = match Self::number(element, "x", 0.0) {
            Some(v) => v,
            None => return false,
        };
        let y = match Self::number(element, "y", 0.0) {
            Some(v) => v,
            None => return false,
        };
        let (width, height) = match (
            element.attributes.get("width").and_then(|v| parse_plain(v)),
            element.attributes.get("height").and_then(|v| parse_plain(v)),
        ) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (w, h),
            _ => return false,
        };
        // Rounded corners need arcs, which this conversion does not produce.
        for corner in ["rx", "ry"] {
            if let Some(value) = element.attributes.get(corner) {
                if parse_plain(value) != Some(0.0) {
                    return false;
                }
            }
        }
        if element.attributes.keys().any(|k| k.starts_with("marker")) || element.attributes.contains_key("style") {
            return false;
        }

        let segments = [
            Segment::Move { x, y },
            Segment::Line { x: x + width, y },
            Segment::Line { x: x + width, y: y + height },
            Segment::Line { x, y: y + height },
            Segment::Close,
        ];
        Self::set_path(
            element,
            &segments,
            &["x", "y", "width", "height", "rx", "ry"],
        );
        true
    }

    fn line_to_path(element: &mut Element) -> bool {
        let x1 = match Self::number(element, "x1", 0.0) {
            Some(v) => v,
            None => return false,
        };
        let y1 = match Self::number(element, "y1", 0.0) {
            Some(v) => v,
            None => return false,
        };
        let x2 = match Self::number(element, "x2", 0.0) {
            Some(v) => v,
            None => return false,
        };
        let y2 = match Self::number(element, "y2", 0.0) {
            Some(v) => v,
            None => return false,
        };

        let segments = [
            Segment::Move { x: x1, y: y1 },
            Segment::Line { x: x2, y: y2 },
        ];
        Self::set_path(element, &segments, &["x1", "y1", "x2", "y2"]);
        true
    }

    fn polygon_to_path(element: &mut Element, close: bool) -> bool {
        let values = match element.attributes.get("points").and_then(|p| parse_list(p)) {
            Some(v) => v,
            None => return false,
        };
        // Per the spec, an odd trailing value is ignored; a point needs two values.
        let pairs: Vec<(f64, f64)> = values.chunks_exact(2).map(|p| (p[0], p[1])).collect();
        if pairs.len() < 2 {
            return false;
        }

        let mut segments = vec![Segment::Move {
            x: pairs[0].0,
            y: pairs[0].1,
        }];
        for &(x, y) in &pairs[1..] {
            segments.push(Segment::Line { x, y });
        }
        if close {
            segments.push(Segment::Close);
        }
        Self::set_path(element, &segments, &["points"]);
        true
    }
}

impl SingleElementPluginTrait for ShapeToPathPlugin {
    fn process(&self, element: &mut Element) -> Result<Element, Box<dyn Error>> {
        let mut element_clone = element.clone();

        match element_clone.name.clone().as_str() {
            "rect" if self.convert_rects => {
                Self::rect_to_path(&mut element_clone);
            }
            "line" => {
                Self::line_to_path(&mut element_clone);
            }
            "polyline" => {
                Self::polygon_to_path(&mut element_clone, false);
            }
            "polygon" => {
                Self::polygon_to_path(&mut element_clone, true);
            }
            _ => {}
        }

        Ok(element_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(xml: &str) -> Element {
        let mut element = Element::parse(xml.as_bytes()).unwrap();
        ShapeToPathPlugin { convert_rects: true }
            .process(&mut element)
            .unwrap()
    }

    #[test]
    fn rect_becomes_closed_path() {
        let el = convert(r##"<rect xmlns="http://www.w3.org/2000/svg" x="10" y="20" width="30" height="40"/>"##);
        assert_eq!(el.name, "path");
        assert_eq!(el.attributes["d"], "M10 20H40V60H10Z");
    }

    #[test]
    fn unit_values_are_left_alone() {
        let el = convert(r##"<rect xmlns="http://www.w3.org/2000/svg" x="10%" width="5" height="5"/>"##);
        assert_eq!(el.name, "rect");
        let el = convert(r##"<line xmlns="http://www.w3.org/2000/svg" x1="5%" y1="0" x2="1" y2="1"/>"##);
        assert_eq!(el.name, "line");
    }

    #[test]
    fn odd_polygon_points_stay_valid() {
        let el = convert(r##"<polygon xmlns="http://www.w3.org/2000/svg" points="1,2 3,4 5"/>"##);
        assert_eq!(el.attributes["d"], "M1 2L3 4Z");
    }

    #[test]
    fn rect_with_marker_is_left_alone() {
        let el = convert(r##"<rect xmlns="http://www.w3.org/2000/svg" width="5" height="5" marker-start="url(#m)"/>"##);
        assert_eq!(el.name, "rect");
    }

    #[test]
    fn line_becomes_path() {
        let el = convert(r##"<line xmlns="http://www.w3.org/2000/svg" x1="0" y1="0" x2="10" y2="5"/>"##);
        assert_eq!(el.attributes["d"], "M0 0L10 5");
    }
}
