// Plugin
//
// Name: Simplify Paths
// Author: Nikola Lazarov
//
// Description:
// Rewrites path data into a shorter form that draws exactly the same thing:
// - Curves whose control points lie on the chord, in order, become lines
// - Arcs with a zero radius become lines, arcs with equal endpoints are dropped
// - Zero-length segments are removed (unless they are the only content of a
//   subpath, where round or square caps still draw a dot)
// - A line back to the subpath start that is immediately closed with Z is dropped
// - Shorthands (H, V, S, T), relative/absolute choice and implicit repetition
//   are applied on output
//
// When markers can be drawn (`keep_vertices`), nothing is removed: markers are
// placed on every vertex, so dropping a segment would drop or move a marker.
// Curves and arcs are still straightened, which keeps their vertices.
//
// Path data that does not parse is left untouched.

use crate::path::{Path, Segment};
use crate::plugin::SingleElementPluginTrait;
use std::error::Error;
use xmltree::Element;

/// Absolute distance under which two coordinates are treated as the same point.
const EPSILON: f64 = 1e-6;

pub struct SimplifyPathsPlugin {
    /// Keep every vertex, because markers may be drawn on them.
    pub keep_vertices: bool,
}

type Point = (f64, f64);

fn same_point(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < EPSILON && (a.1 - b.1).abs() < EPSILON
}

/// True when a cubic or quadratic curve from `start` through `controls` to `end`
/// is a straight line that it traces exactly once, from start to end.
///
/// That holds when every control point lies on the chord and the control
/// points are ordered along it. Collinear control points that overshoot the
/// chord are not straight: the curve doubles back and paints more than the line.
fn is_straight(start: Point, controls: &[Point], end: Point) -> bool {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = (dx * dx + dy * dy).sqrt();
    if length < EPSILON {
        return false;
    }
    let (ux, uy) = (dx / length, dy / length);

    let mut last = 0.0f64;
    for &(px, py) in controls {
        let rx = px - start.0;
        let ry = py - start.1;
        let along = rx * ux + ry * uy;
        let across = rx * uy - ry * ux;
        if across.abs() > EPSILON {
            return false;
        }
        if along < -EPSILON || along > length + EPSILON || along < last - EPSILON {
            return false;
        }
        last = along;
    }
    true
}

/// Converts curves and arcs that draw a straight line into `Line` segments.
fn straighten(segments: &[Segment]) -> Vec<Segment> {
    let mut out = Vec::with_capacity(segments.len());
    let mut cur: Point = (0.0, 0.0);
    let mut subpath_start: Point = (0.0, 0.0);

    for seg in segments {
        let replacement = match *seg {
            Segment::Cubic { x1, y1, x2, y2, x, y } => {
                if is_straight(cur, &[(x1, y1), (x2, y2)], (x, y)) {
                    Segment::Line { x, y }
                } else {
                    *seg
                }
            }
            Segment::Quad { x1, y1, x, y } => {
                if is_straight(cur, &[(x1, y1)], (x, y)) {
                    Segment::Line { x, y }
                } else {
                    *seg
                }
            }
            // An arc with a zero radius is drawn as a straight line.
            Segment::Arc { rx, ry, x, y, .. } if rx == 0.0 || ry == 0.0 => Segment::Line { x, y },
            _ => *seg,
        };

        if let Segment::Move { x, y } = replacement {
            subpath_start = (x, y);
        }
        cur = replacement.end_point(subpath_start);
        out.push(replacement);
    }

    out
}

/// True when a segment starts and ends at `cur` and draws nothing.
fn is_zero_length(seg: &Segment, cur: Point) -> bool {
    match *seg {
        Segment::Line { x, y } => same_point((x, y), cur),
        Segment::Cubic { x1, y1, x2, y2, x, y } => {
            same_point((x, y), cur) && same_point((x1, y1), cur) && same_point((x2, y2), cur)
        }
        Segment::Quad { x1, y1, x, y } => {
            same_point((x, y), cur) && same_point((x1, y1), cur)
        }
        // An arc whose endpoints coincide is omitted by the spec.
        Segment::Arc { x, y, .. } => same_point((x, y), cur),
        _ => false,
    }
}

/// Removes segments that draw nothing and lines that a `Z` already draws.
fn remove_redundant(segments: Vec<Segment>) -> Vec<Segment> {
    // Split into subpaths so the "only content" rule can be applied per subpath.
    let mut subpaths: Vec<Vec<Segment>> = Vec::new();
    for seg in segments {
        if let Segment::Move { .. } = seg {
            subpaths.push(vec![seg]);
        } else if let Some(current) = subpaths.last_mut() {
            current.push(seg);
        } else {
            subpaths.push(vec![seg]);
        }
    }

    let mut out = Vec::new();
    for subpath in subpaths {
        // Replay the subpath to know where each segment starts.
        let mut cur: Point = (0.0, 0.0);
        let mut start: Point = (0.0, 0.0);
        let mut zero_flags = Vec::with_capacity(subpath.len());
        for seg in &subpath {
            zero_flags.push(is_zero_length(seg, cur));
            if let Segment::Move { x, y } = *seg {
                start = (x, y);
            }
            cur = seg.end_point(start);
        }

        // A subpath whose only drawing is zero-length keeps it, since caps draw.
        let has_real_drawing = subpath
            .iter()
            .zip(&zero_flags)
            .any(|(seg, &zero)| !zero && !matches!(seg, Segment::Move { .. } | Segment::Close));
        let keep_zero = !has_real_drawing;

        let mut kept: Vec<Segment> = Vec::with_capacity(subpath.len());
        let mut cur: Point = (0.0, 0.0);
        let mut start: Point = (0.0, 0.0);
        for (index, seg) in subpath.iter().enumerate() {
            let zero = zero_flags[index];
            let at_start_before = same_point(cur, start);
            if let Segment::Move { x, y } = *seg {
                start = (x, y);
            }

            let drop_zero = zero && !keep_zero && !matches!(seg, Segment::Move { .. });
            // A line to the start point followed by Z adds nothing: Z draws it.
            let drop_closing_line = matches!(seg, Segment::Line { .. })
                && same_point(seg.end_point(start), start)
                && !at_start_before
                && matches!(subpath.get(index + 1), Some(Segment::Close));

            if !drop_zero && !drop_closing_line {
                kept.push(*seg);
            }
            cur = seg.end_point(start);
        }
        out.extend(kept);
    }

    out
}

/// Runs the optimization passes over absolute segments.
fn optimize(segments: &[Segment], keep_vertices: bool) -> Vec<Segment> {
    let straight = straighten(segments);
    if keep_vertices {
        straight
    } else {
        remove_redundant(straight)
    }
}

impl SimplifyPathsPlugin {
    fn simplify(d: &str, keep_vertices: bool) -> Option<String> {
        let path = Path::parse(d)?;
        let segments = path.segments()?;
        if segments.is_empty() {
            return None;
        }
        Some(Path::emit(&optimize(&segments, keep_vertices), None))
    }
}

impl SingleElementPluginTrait for SimplifyPathsPlugin {
    fn process(&self, element: &mut Element) -> Result<Element, Box<dyn Error>> {
        let mut element_clone = element.clone();

        // Only elements with path data; anything malformed is left alone.
        if let Some(d) = element_clone.attributes.get("d").cloned() {
            let has_own_markers = element_clone.attributes.keys().any(|k| k.starts_with("marker"));
            if let Some(simplified) = Self::simplify(&d, self.keep_vertices || has_own_markers) {
                // Keep the original spelling if the rewrite is not shorter.
                if simplified.len() < d.len() {
                    element_clone.attributes.insert("d".to_string(), simplified);
                }
            }
        }

        Ok(element_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simplify(d: &str) -> String {
        SimplifyPathsPlugin::simplify(d, false).expect("valid path")
    }

    fn drawn(d: &str) -> Vec<Segment> {
        optimize(&Path::parse(d).unwrap().segments().unwrap(), false)
    }

    #[test]
    fn collinear_overshoot_is_not_straightened() {
        // Control points lie on the x-axis but the first one lies beyond the end
        // point, so the curve doubles back. It must stay a curve.
        let segs = drawn("M30 60C190 60 -60 60 120 60");
        assert!(matches!(segs[1], Segment::Cubic { .. }));
    }

    #[test]
    fn ordered_collinear_curve_becomes_line() {
        let segs = drawn("M0 0C10 0 20 0 30 0");
        assert!(matches!(segs[1], Segment::Line { .. }));
    }

    #[test]
    fn multi_segment_commands_are_not_truncated() {
        // Every segment of a multi-pair command must survive.
        let out = simplify("M10 10L50 10 90 10 90 50");
        let segs = Path::parse(&out).unwrap().segments().unwrap();
        assert_eq!(segs.len(), 4);
        match segs[3] {
            Segment::Line { x, y } => assert!((x - 90.0).abs() < 1e-9 && (y - 50.0).abs() < 1e-9),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn multi_curve_command_is_not_truncated() {
        let d = "M0 0c10 -10 20 -10 30 0 10 10 20 10 30 0";
        let before = Path::parse(d).unwrap().segments().unwrap();
        let after = drawn(d);
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn zero_length_segment_kept_when_only_content() {
        // A lone zero-length segment draws a dot with round caps; keep it.
        let segs = drawn("M10 10 L10 10");
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn zero_length_segment_removed_mid_subpath() {
        let segs = drawn("M0 0L10 0L10 0L20 0");
        assert_eq!(segs.len(), 3);
    }

    #[test]
    fn zero_length_smooth_segment_does_not_change_shape() {
        // A zero-length s followed by S must render the same as before.
        let d = "M20 100C40 40 60 40 80 100s0 0 0 0S140 160 160 100";
        let before = Path::parse(d).unwrap().segments().unwrap();
        let after = Path::parse(&simplify(d)).unwrap().segments().unwrap();
        let end_points = |segs: &[Segment]| {
            segs.iter()
                .filter_map(|s| match s {
                    Segment::Cubic { x, y, .. } => Some((*x, *y)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(end_points(&before).len(), end_points(&after).len());
        assert_eq!(before.last(), after.last());
    }

    #[test]
    fn keeps_every_vertex_when_markers_are_possible() {
        let d = "M0 0L10 0L10 0L10 10L0 0Z";
        let before = Path::parse(d).unwrap().segments().unwrap();
        let after = optimize(&before, true);
        assert_eq!(before.len(), after.len());
    }

    #[test]
    fn closing_line_is_dropped_before_z() {
        let out = simplify("M0 0L10 0L10 10L0 0Z");
        assert!(out.ends_with("Z"));
        assert_eq!(drawn("M0 0L10 0L10 10L0 0Z").len(), 4);
    }

    #[test]
    fn invalid_path_is_untouched() {
        assert!(SimplifyPathsPlugin::simplify("M10", false).is_none());
    }

    #[test]
    fn output_is_never_longer_than_input() {
        let mut element = Element::new("path");
        element
            .attributes
            .insert("d".to_string(), "M 10.0 10.0 L 20.0 10.0".to_string());
        let out = SimplifyPathsPlugin { keep_vertices: false }
            .process(&mut element)
            .unwrap();
        assert!(out.attributes["d"].len() <= "M 10.0 10.0 L 20.0 10.0".len());
    }
}
