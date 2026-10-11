use crate::helpers::tree::num_attr;
use crate::helpers::number::{parse_list, parse_plain};
use crate::path::{Path, Segment};
use xmltree::Element;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BBox {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        BBox {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.max_x >= self.min_x && self.max_y >= self.min_y
    }

    /// Returns true if `self` fully contains `other`.
    pub fn contains(&self, other: &BBox) -> bool {
        self.min_x <= other.min_x
            && self.min_y <= other.min_y
            && self.max_x >= other.max_x
            && self.max_y >= other.max_y
    }

    /// Returns true if the two boxes share any area or touch.
    pub fn overlaps(&self, other: &BBox) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    pub fn union(&self, other: &BBox) -> BBox {
        BBox {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn expand_to(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
}

fn required_attr(element: &Element, name: &str) -> Option<f64> {
    element.attributes.get(name).and_then(|v| parse_plain(v))
}

fn bbox_rect(element: &Element) -> Option<BBox> {
    let x = num_attr(element, "x", 0.0)?;
    let y = num_attr(element, "y", 0.0)?;
    let width = required_attr(element, "width")?;
    let height = required_attr(element, "height")?;
    if width < 0.0 || height < 0.0 {
        return None;
    }
    // Rounded corners only remove area, so the sharp rectangle is an upper bound.
    Some(BBox::new(x, y, x + width, y + height))
}

fn bbox_circle(element: &Element) -> Option<BBox> {
    let cx = num_attr(element, "cx", 0.0)?;
    let cy = num_attr(element, "cy", 0.0)?;
    let r = required_attr(element, "r")?;
    if r < 0.0 {
        return None;
    }
    Some(BBox::new(cx - r, cy - r, cx + r, cy + r))
}

fn bbox_ellipse(element: &Element) -> Option<BBox> {
    let cx = num_attr(element, "cx", 0.0)?;
    let cy = num_attr(element, "cy", 0.0)?;
    let rx = required_attr(element, "rx")?;
    let ry = required_attr(element, "ry")?;
    if rx < 0.0 || ry < 0.0 {
        return None;
    }
    Some(BBox::new(cx - rx, cy - ry, cx + rx, cy + ry))
}

fn bbox_line(element: &Element) -> Option<BBox> {
    let x1 = num_attr(element, "x1", 0.0)?;
    let y1 = num_attr(element, "y1", 0.0)?;
    let x2 = num_attr(element, "x2", 0.0)?;
    let y2 = num_attr(element, "y2", 0.0)?;
    Some(BBox::new(x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)))
}

/// Bounding box of a `points` list. An odd count or a single point is treated as
/// unknown.
fn bbox_points(element: &Element) -> Option<BBox> {
    let nums = parse_list(element.attributes.get("points")?)?;
    if nums.len() < 4 || nums.len() % 2 != 0 {
        return None;
    }
    let mut bbox = BBox::new(nums[0], nums[1], nums[0], nums[1]);
    for pair in nums.chunks(2) {
        bbox.expand_to(pair[0], pair[1]);
    }
    Some(bbox)
}

/// Bounding box of path segments that is guaranteed to contain the painted
/// geometry. Curves contribute their control points, which bounds the curve.
/// An arc lies within `2 * a` of its start point, where `a` is its (possibly
/// scaled up) larger radius, so that square is used.
pub fn segments_bbox(segments: &[Segment]) -> Option<BBox> {
    let mut bbox: Option<BBox> = None;
    let mut add = |x: f64, y: f64| match bbox.as_mut() {
        Some(b) => b.expand_to(x, y),
        None => bbox = Some(BBox::new(x, y, x, y)),
    };

    let mut cur = (0.0f64, 0.0f64);
    let mut start = (0.0f64, 0.0f64);
    for seg in segments {
        match *seg {
            Segment::Move { x, y } => {
                add(x, y);
                start = (x, y);
            }
            Segment::Line { x, y } => add(x, y),
            Segment::Cubic { x1, y1, x2, y2, x, y } => {
                add(x1, y1);
                add(x2, y2);
                add(x, y);
            }
            Segment::Quad { x1, y1, x, y } => {
                add(x1, y1);
                add(x, y);
            }
            Segment::Arc { rx, ry, rotation, x, y, .. } => {
                add(x, y);
                let (rx, ry) = (rx.abs(), ry.abs());
                if rx > 0.0 && ry > 0.0 {
                    // Half-chord in the ellipse's rotated frame, used to find the
                    // radius the spec scales up to when the radii are too small.
                    let phi = rotation.to_radians();
                    let (sin, cos) = phi.sin_cos();
                    let dx = (cur.0 - x) / 2.0;
                    let dy = (cur.1 - y) / 2.0;
                    let x1p = cos * dx + sin * dy;
                    let y1p = -sin * dx + cos * dy;
                    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
                    let a = rx.max(ry) * lambda.max(1.0).sqrt();
                    let bound = 2.0 * a;
                    add(cur.0 - bound, cur.1 - bound);
                    add(cur.0 + bound, cur.1 + bound);
                }
            }
            Segment::Close => {}
        }
        cur = seg.end_point(start);
    }

    bbox.filter(|b| b.is_valid())
}

fn bbox_path(element: &Element) -> Option<BBox> {
    let segments = Path::parse(element.attributes.get("d")?)?.segments()?;
    segments_bbox(&segments)
}

/// Bounding box of a group: the union of its children, ignoring children that
/// are not displayed. A child with its own transform is unknown.
fn bbox_group(element: &Element) -> Option<BBox> {
    let mut result: Option<BBox> = None;
    for child in element.children.iter().filter_map(|c| c.as_element()) {
        if child.attributes.get("display").map(String::as_str) == Some("none") {
            continue;
        }
        if child.attributes.contains_key("transform") {
            return None;
        }
        let child_bbox = compute_element_bbox(child)?;
        result = Some(match result {
            Some(existing) => existing.union(&child_bbox),
            None => child_bbox,
        });
    }
    result
}

/// Bounding box of an element's geometry in its own user space.
///
/// The element's own `transform` is not applied: clip paths and similar
/// references are in that same user space, so local coordinates are the right
/// frame. A child with its own transform makes the result unknown.
///
/// Returns `None` whenever the geometry cannot be bounded from the attributes
/// alone (text, images, `use`, unparseable values).
pub fn compute_element_bbox(element: &Element) -> Option<BBox> {
    match element.name.as_str() {
        "rect" => bbox_rect(element),
        "circle" => bbox_circle(element),
        "ellipse" => bbox_ellipse(element),
        "line" => bbox_line(element),
        "polyline" | "polygon" => bbox_points(element),
        "path" => bbox_path(element),
        "g" | "a" | "clipPath" => bbox_group(element),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unparseable_attribute_is_unknown_not_zero() {
        let mut rect = Element::new("rect");
        rect.attributes.insert("x".into(), "10%".into());
        rect.attributes.insert("width".into(), "5".into());
        rect.attributes.insert("height".into(), "5".into());
        assert!(compute_element_bbox(&rect).is_none());
    }

    #[test]
    fn curve_bbox_includes_control_points() {
        let segs = Path::parse("M0 0C0 100 100 100 100 0").unwrap().segments().unwrap();
        let b = segments_bbox(&segs).unwrap();
        assert_eq!(b.max_y, 100.0);
    }

    #[test]
    fn arc_bbox_covers_bulge() {
        // A half circle of radius 50 from (0,0) to (100,0) bulges to y=50 (or -50).
        let segs = Path::parse("M0 0A50 50 0 0 1 100 0").unwrap().segments().unwrap();
        let b = segments_bbox(&segs).unwrap();
        assert!(b.min_y <= -50.0 + 1e-9 || b.max_y >= 50.0 - 1e-9);
    }
}
