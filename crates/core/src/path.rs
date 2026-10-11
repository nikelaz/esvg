/*
 * Path Data Representation
 * Author: Nikola Lazarov
 *
 * Three layers:
 * - `Path`: the command list exactly as written in `d`, parsed strictly.
 * - `Segment`: the same geometry as a list of absolute segments, which is what
 *   the path-rewriting plugins operate on.
 * - `Path::emit` / `Path::to_string`: compact serialization back to `d`.
 */

use crate::helpers::number::{format_number, scan_number, skip_separators};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PathCommandType {
    MoveTo,
    MoveToRelative,
    LineTo,
    LineToRelative,
    HorizontalLine,
    HorizontalLineRelative,
    VerticalLine,
    VerticalLineRelative,
    CubicBezierCurve,
    CubicBezierCurveRelative,
    AdditionalBezierCurve,
    AdditionalBezierCurveRelative,
    QuadraticBezierCurve,
    QuadraticBezierCurveRelative,
    AdditionalQuadraticBezierCurve,
    AdditionalQuadraticBezierCurveRelative,
    Arc,
    ArcRelative,
    Close,
    CloseAlternate,
}

impl PathCommandType {
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'M' => PathCommandType::MoveTo,
            'm' => PathCommandType::MoveToRelative,
            'L' => PathCommandType::LineTo,
            'l' => PathCommandType::LineToRelative,
            'H' => PathCommandType::HorizontalLine,
            'h' => PathCommandType::HorizontalLineRelative,
            'V' => PathCommandType::VerticalLine,
            'v' => PathCommandType::VerticalLineRelative,
            'C' => PathCommandType::CubicBezierCurve,
            'c' => PathCommandType::CubicBezierCurveRelative,
            'S' => PathCommandType::AdditionalBezierCurve,
            's' => PathCommandType::AdditionalBezierCurveRelative,
            'Q' => PathCommandType::QuadraticBezierCurve,
            'q' => PathCommandType::QuadraticBezierCurveRelative,
            'T' => PathCommandType::AdditionalQuadraticBezierCurve,
            't' => PathCommandType::AdditionalQuadraticBezierCurveRelative,
            'A' => PathCommandType::Arc,
            'a' => PathCommandType::ArcRelative,
            'Z' => PathCommandType::Close,
            'z' => PathCommandType::CloseAlternate,
            _ => return None,
        })
    }

    pub fn to_char(&self) -> char {
        match self {
            PathCommandType::MoveTo => 'M',
            PathCommandType::MoveToRelative => 'm',
            PathCommandType::LineTo => 'L',
            PathCommandType::LineToRelative => 'l',
            PathCommandType::HorizontalLine => 'H',
            PathCommandType::HorizontalLineRelative => 'h',
            PathCommandType::VerticalLine => 'V',
            PathCommandType::VerticalLineRelative => 'v',
            PathCommandType::CubicBezierCurve => 'C',
            PathCommandType::CubicBezierCurveRelative => 'c',
            PathCommandType::AdditionalBezierCurve => 'S',
            PathCommandType::AdditionalBezierCurveRelative => 's',
            PathCommandType::QuadraticBezierCurve => 'Q',
            PathCommandType::QuadraticBezierCurveRelative => 'q',
            PathCommandType::AdditionalQuadraticBezierCurve => 'T',
            PathCommandType::AdditionalQuadraticBezierCurveRelative => 't',
            PathCommandType::Arc => 'A',
            PathCommandType::ArcRelative => 'a',
            PathCommandType::Close => 'Z',
            PathCommandType::CloseAlternate => 'z',
        }
    }

    /// Number of arguments that make up one segment of this command.
    pub fn arity(&self) -> usize {
        match self {
            PathCommandType::Close | PathCommandType::CloseAlternate => 0,
            PathCommandType::HorizontalLine
            | PathCommandType::HorizontalLineRelative
            | PathCommandType::VerticalLine
            | PathCommandType::VerticalLineRelative => 1,
            PathCommandType::MoveTo
            | PathCommandType::MoveToRelative
            | PathCommandType::LineTo
            | PathCommandType::LineToRelative
            | PathCommandType::AdditionalQuadraticBezierCurve
            | PathCommandType::AdditionalQuadraticBezierCurveRelative => 2,
            PathCommandType::AdditionalBezierCurve
            | PathCommandType::AdditionalBezierCurveRelative
            | PathCommandType::QuadraticBezierCurve
            | PathCommandType::QuadraticBezierCurveRelative => 4,
            PathCommandType::CubicBezierCurve | PathCommandType::CubicBezierCurveRelative => 6,
            PathCommandType::Arc | PathCommandType::ArcRelative => 7,
        }
    }

    pub fn is_relative(&self) -> bool {
        self.to_char().is_ascii_lowercase()
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct PathCommand {
    pub command_type: PathCommandType,
    pub values: Vec<f32>,
}

/// One drawing step in absolute coordinates.
///
/// Relative commands, shorthands (`H`, `V`, `S`, `T`), and implicit repetitions
/// are all resolved into this form, so the plugins never have to reason about
/// how a segment was written.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Segment {
    Move { x: f64, y: f64 },
    Line { x: f64, y: f64 },
    Cubic { x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64 },
    Quad { x1: f64, y1: f64, x: f64, y: f64 },
    /// `large` and `sweep` are 0.0 or 1.0, as written in the path data.
    Arc { rx: f64, ry: f64, rotation: f64, large: f64, sweep: f64, x: f64, y: f64 },
    Close,
}

impl Segment {
    /// The point the pen is at after this segment. `subpath_start` is where the
    /// current subpath began, which is where a close returns to.
    pub fn end_point(&self, subpath_start: (f64, f64)) -> (f64, f64) {
        match *self {
            Segment::Move { x, y }
            | Segment::Line { x, y }
            | Segment::Cubic { x, y, .. }
            | Segment::Quad { x, y, .. }
            | Segment::Arc { x, y, .. } => (x, y),
            Segment::Close => subpath_start,
        }
    }
}

#[derive(Debug)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    /// Strictly parses SVG path data.
    ///
    /// Returns `None` for anything a renderer would reject: an unknown
    /// character, a missing or extra argument, data before the first command,
    /// or a flag other than `0`/`1`. Callers should leave the original `d`
    /// untouched in that case.
    pub fn parse(d: &str) -> Option<Path> {
        let mut commands: Vec<PathCommand> = Vec::new();
        let mut i = skip_separators(d, 0);
        // A command that takes arguments must have at least one full set.
        let complete = |command: Option<&PathCommand>| {
            command.map_or(true, |c| c.command_type.arity() == 0 || !c.values.is_empty())
        };

        while i < d.len() {
            let c = d[i..].chars().next()?;
            if let Some(command_type) = PathCommandType::from_char(c) {
                if !complete(commands.last()) {
                    return None;
                }
                commands.push(PathCommand {
                    command_type,
                    values: Vec::new(),
                });
                i = skip_separators(d, i + 1);
                continue;
            }

            let current = commands.last_mut()?;
            let arity = current.command_type.arity();
            if arity == 0 {
                return None;
            }
            // Arc flags are a single `0` or `1` and may be written without a
            // separator, so they cannot go through the general number scanner.
            let is_arc = matches!(
                current.command_type,
                PathCommandType::Arc | PathCommandType::ArcRelative
            );

            for index in 0..arity {
                i = skip_separators(d, i);
                if is_arc && (index == 3 || index == 4) {
                    match d.as_bytes().get(i) {
                        Some(b'0') => current.values.push(0.0),
                        Some(b'1') => current.values.push(1.0),
                        _ => return None,
                    }
                    i += 1;
                } else {
                    let (value, next) = scan_number(d, i)?;
                    current.values.push(value as f32);
                    i = next;
                }
            }
            i = skip_separators(d, i);
        }

        if !complete(commands.last()) {
            return None;
        }

        // Path data must begin with a moveto. An empty string is valid and draws nothing.
        if let Some(first) = commands.first() {
            if !matches!(
                first.command_type,
                PathCommandType::MoveTo | PathCommandType::MoveToRelative
            ) {
                return None;
            }
        }

        Some(Path { commands })
    }

    /// Converts the path to absolute segments.
    ///
    /// Returns `None` if the path does not start with a moveto, since such data
    /// is not a valid path.
    pub fn segments(&self) -> Option<Vec<Segment>> {
        if let Some(first) = self.commands.first() {
            if !matches!(
                first.command_type,
                PathCommandType::MoveTo | PathCommandType::MoveToRelative
            ) {
                return None;
            }
        }

        let mut out: Vec<Segment> = Vec::new();
        let (mut cx, mut cy) = (0.0f64, 0.0f64);
        let (mut sx, mut sy) = (0.0f64, 0.0f64);
        // Control points of the previous segment, used for S and T reflection.
        let mut prev_cubic: Option<(f64, f64)> = None;
        let mut prev_quad: Option<(f64, f64)> = None;

        for command in &self.commands {
            let v: Vec<f64> = command.values.iter().map(|&x| x as f64).collect();
            let rel = command.command_type.is_relative();

            match command.command_type {
                PathCommandType::MoveTo | PathCommandType::MoveToRelative => {
                    for (index, pair) in v.chunks_exact(2).enumerate() {
                        let x = if rel { cx + pair[0] } else { pair[0] };
                        let y = if rel { cy + pair[1] } else { pair[1] };
                        // Extra pairs after a moveto are implicit linetos.
                        if index == 0 {
                            out.push(Segment::Move { x, y });
                            sx = x;
                            sy = y;
                        } else {
                            out.push(Segment::Line { x, y });
                        }
                        cx = x;
                        cy = y;
                        prev_cubic = None;
                        prev_quad = None;
                    }
                }
                PathCommandType::LineTo | PathCommandType::LineToRelative => {
                    for pair in v.chunks_exact(2) {
                        let x = if rel { cx + pair[0] } else { pair[0] };
                        let y = if rel { cy + pair[1] } else { pair[1] };
                        out.push(Segment::Line { x, y });
                        cx = x;
                        cy = y;
                        prev_cubic = None;
                        prev_quad = None;
                    }
                }
                PathCommandType::HorizontalLine | PathCommandType::HorizontalLineRelative => {
                    for &value in &v {
                        let x = if rel { cx + value } else { value };
                        out.push(Segment::Line { x, y: cy });
                        cx = x;
                        prev_cubic = None;
                        prev_quad = None;
                    }
                }
                PathCommandType::VerticalLine | PathCommandType::VerticalLineRelative => {
                    for &value in &v {
                        let y = if rel { cy + value } else { value };
                        out.push(Segment::Line { x: cx, y });
                        cy = y;
                        prev_cubic = None;
                        prev_quad = None;
                    }
                }
                PathCommandType::CubicBezierCurve | PathCommandType::CubicBezierCurveRelative => {
                    for g in v.chunks_exact(6) {
                        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                        let seg = Segment::Cubic {
                            x1: ox + g[0],
                            y1: oy + g[1],
                            x2: ox + g[2],
                            y2: oy + g[3],
                            x: ox + g[4],
                            y: oy + g[5],
                        };
                        if let Segment::Cubic { x2, y2, x, y, .. } = seg {
                            prev_cubic = Some((x2, y2));
                            prev_quad = None;
                            cx = x;
                            cy = y;
                        }
                        out.push(seg);
                    }
                }
                PathCommandType::AdditionalBezierCurve
                | PathCommandType::AdditionalBezierCurveRelative => {
                    for g in v.chunks_exact(4) {
                        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                        let (x1, y1) = match prev_cubic {
                            Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                            None => (cx, cy),
                        };
                        let x2 = ox + g[0];
                        let y2 = oy + g[1];
                        let x = ox + g[2];
                        let y = oy + g[3];
                        out.push(Segment::Cubic { x1, y1, x2, y2, x, y });
                        prev_cubic = Some((x2, y2));
                        prev_quad = None;
                        cx = x;
                        cy = y;
                    }
                }
                PathCommandType::QuadraticBezierCurve | PathCommandType::QuadraticBezierCurveRelative => {
                    for g in v.chunks_exact(4) {
                        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                        let x1 = ox + g[0];
                        let y1 = oy + g[1];
                        let x = ox + g[2];
                        let y = oy + g[3];
                        out.push(Segment::Quad { x1, y1, x, y });
                        prev_quad = Some((x1, y1));
                        prev_cubic = None;
                        cx = x;
                        cy = y;
                    }
                }
                PathCommandType::AdditionalQuadraticBezierCurve
                | PathCommandType::AdditionalQuadraticBezierCurveRelative => {
                    for g in v.chunks_exact(2) {
                        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                        let (x1, y1) = match prev_quad {
                            Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                            None => (cx, cy),
                        };
                        let x = ox + g[0];
                        let y = oy + g[1];
                        out.push(Segment::Quad { x1, y1, x, y });
                        prev_quad = Some((x1, y1));
                        prev_cubic = None;
                        cx = x;
                        cy = y;
                    }
                }
                PathCommandType::Arc | PathCommandType::ArcRelative => {
                    for g in v.chunks_exact(7) {
                        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                        let x = ox + g[5];
                        let y = oy + g[6];
                        out.push(Segment::Arc {
                            rx: g[0],
                            ry: g[1],
                            rotation: g[2],
                            large: g[3],
                            sweep: g[4],
                            x,
                            y,
                        });
                        prev_cubic = None;
                        prev_quad = None;
                        cx = x;
                        cy = y;
                    }
                }
                PathCommandType::Close | PathCommandType::CloseAlternate => {
                    out.push(Segment::Close);
                    cx = sx;
                    cy = sy;
                    prev_cubic = None;
                    prev_quad = None;
                }
            }
        }

        Some(out)
    }

    /// Serializes segments back to path data.
    ///
    /// Each segment is written in whichever of absolute or relative form is
    /// shorter, using `H`/`V`/`S`/`T` where they apply. Consecutive segments of
    /// the same command share one letter. If `decimals` is set, absolute
    /// coordinates are snapped to that many decimal places and relative offsets
    /// are computed from the snapped positions, so rounding error does not
    /// accumulate along the path.
    pub fn emit(segments: &[Segment], decimals: Option<u32>) -> String {
        let snap = |v: f64| match decimals {
            Some(d) => {
                let f = 10f64.powi(d as i32);
                (v * f).round() / f
            }
            None => v,
        };
        let eq = |a: f64, b: f64| (a - b).abs() < 1e-9;

        let mut commands: Vec<PathCommand> = Vec::new();
        let (mut cx, mut cy) = (0.0f64, 0.0f64);
        let (mut sx, mut sy) = (0.0f64, 0.0f64);
        // Previous segment's second control point (cubic) or control point (quad),
        // in snapped coordinates, if the previous segment was of that kind.
        let mut prev_cubic: Option<(f64, f64)> = None;
        let mut prev_quad: Option<(f64, f64)> = None;

        for seg in segments {
            // Each candidate is (command type, values). Absolute and relative forms
            // are built side by side and the shorter one is kept.
            let (abs_cmd, rel_cmd, values_abs, values_rel, next) = match *seg {
                Segment::Move { x, y } => {
                    let (x, y) = (snap(x), snap(y));
                    (
                        PathCommandType::MoveTo,
                        PathCommandType::MoveToRelative,
                        vec![x, y],
                        vec![x - cx, y - cy],
                        (x, y),
                    )
                }
                Segment::Line { x, y } => {
                    let (x, y) = (snap(x), snap(y));
                    if eq(y, cy) {
                        (
                            PathCommandType::HorizontalLine,
                            PathCommandType::HorizontalLineRelative,
                            vec![x],
                            vec![x - cx],
                            (x, cy),
                        )
                    } else if eq(x, cx) {
                        (
                            PathCommandType::VerticalLine,
                            PathCommandType::VerticalLineRelative,
                            vec![y],
                            vec![y - cy],
                            (cx, y),
                        )
                    } else {
                        (
                            PathCommandType::LineTo,
                            PathCommandType::LineToRelative,
                            vec![x, y],
                            vec![x - cx, y - cy],
                            (x, y),
                        )
                    }
                }
                Segment::Cubic { x1, y1, x2, y2, x, y } => {
                    let (x1, y1, x2, y2, x, y) = (snap(x1), snap(y1), snap(x2), snap(y2), snap(x), snap(y));
                    let reflected = prev_cubic
                        .map(|(px, py)| eq(x1, 2.0 * cx - px) && eq(y1, 2.0 * cy - py))
                        .unwrap_or(false)
                        || (prev_cubic.is_none() && eq(x1, cx) && eq(y1, cy));
                    if reflected {
                        (
                            PathCommandType::AdditionalBezierCurve,
                            PathCommandType::AdditionalBezierCurveRelative,
                            vec![x2, y2, x, y],
                            vec![x2 - cx, y2 - cy, x - cx, y - cy],
                            (x, y),
                        )
                    } else {
                        (
                            PathCommandType::CubicBezierCurve,
                            PathCommandType::CubicBezierCurveRelative,
                            vec![x1, y1, x2, y2, x, y],
                            vec![x1 - cx, y1 - cy, x2 - cx, y2 - cy, x - cx, y - cy],
                            (x, y),
                        )
                    }
                }
                Segment::Quad { x1, y1, x, y } => {
                    let (x1, y1, x, y) = (snap(x1), snap(y1), snap(x), snap(y));
                    let reflected = prev_quad
                        .map(|(px, py)| eq(x1, 2.0 * cx - px) && eq(y1, 2.0 * cy - py))
                        .unwrap_or(false)
                        || (prev_quad.is_none() && eq(x1, cx) && eq(y1, cy));
                    if reflected {
                        (
                            PathCommandType::AdditionalQuadraticBezierCurve,
                            PathCommandType::AdditionalQuadraticBezierCurveRelative,
                            vec![x, y],
                            vec![x - cx, y - cy],
                            (x, y),
                        )
                    } else {
                        (
                            PathCommandType::QuadraticBezierCurve,
                            PathCommandType::QuadraticBezierCurveRelative,
                            vec![x1, y1, x, y],
                            vec![x1 - cx, y1 - cy, x - cx, y - cy],
                            (x, y),
                        )
                    }
                }
                Segment::Arc { rx, ry, rotation, large, sweep, x, y } => {
                    let (x, y) = (snap(x), snap(y));
                    let (rx, ry, rotation) = (snap(rx), snap(ry), snap(rotation));
                    (
                        PathCommandType::Arc,
                        PathCommandType::ArcRelative,
                        vec![rx, ry, rotation, large, sweep, x, y],
                        vec![rx, ry, rotation, large, sweep, x - cx, y - cy],
                        (x, y),
                    )
                }
                Segment::Close => {
                    commands.push(PathCommand {
                        command_type: PathCommandType::Close,
                        values: Vec::new(),
                    });
                    cx = sx;
                    cy = sy;
                    prev_cubic = None;
                    prev_quad = None;
                    continue;
                }
            };

            let abs_len: usize = values_abs.iter().map(|v| format_number(*v).len() + 1).sum();
            let rel_len: usize = values_rel.iter().map(|v| format_number(*v).len() + 1).sum();
            let (command_type, values) = if rel_len < abs_len {
                (rel_cmd, values_rel)
            } else {
                (abs_cmd, values_abs)
            };

            // Implicit repetition: append to the previous command when it is the
            // same command. A moveto never repeats, since extra pairs would
            // then be read as linetos.
            let repeats_previous = commands.last().map_or(false, |last| {
                last.command_type == command_type
                    && !matches!(
                        command_type,
                        PathCommandType::MoveTo | PathCommandType::MoveToRelative
                    )
            });
            if repeats_previous {
                commands.last_mut().unwrap().values.extend(values.iter().map(|v| *v as f32));
            } else {
                commands.push(PathCommand {
                    command_type,
                    values: values.iter().map(|v| *v as f32).collect(),
                });
            }

            // Track the state needed to choose shorthands for the next segment.
            match *seg {
                Segment::Move { .. } => {
                    sx = next.0;
                    sy = next.1;
                    prev_cubic = None;
                    prev_quad = None;
                }
                Segment::Cubic { x2, y2, .. } => {
                    prev_cubic = Some((snap(x2), snap(y2)));
                    prev_quad = None;
                }
                Segment::Quad { x1, y1, .. } => {
                    prev_quad = Some((snap(x1), snap(y1)));
                    prev_cubic = None;
                }
                _ => {
                    prev_cubic = None;
                    prev_quad = None;
                }
            }
            cx = next.0;
            cy = next.1;
        }

        Path { commands }.to_string()
    }

    /// Serializes the command list as written, with compact spacing.
    pub fn to_string(&self) -> String {
        let mut out = String::new();
        for command in &self.commands {
            out.push(command.command_type.to_char());
            let is_arc = matches!(
                command.command_type,
                PathCommandType::Arc | PathCommandType::ArcRelative
            );
            for (index, value) in command.values.iter().enumerate() {
                // In an arc, the sweep flag (position 4) and the endpoint x (position 5)
                // follow a single-character flag, so no separator is needed before them.
                let after_flag = is_arc && matches!(index % 7, 4 | 5);
                push_number(&mut out, *value, !after_flag);
            }
        }
        out
    }
}

/// Appends a number. A separating space is added only where the grammar needs one:
/// a minus sign already separates, and a letter needs none.
fn push_number(out: &mut String, value: f32, may_separate: bool) {
    let text = format_number(value as f64);
    let needs_separator = may_separate
        && match out.chars().last() {
            None => false,
            Some(c) if c.is_ascii_alphabetic() => false,
            Some(_) => !text.starts_with('-'),
        };
    if needs_separator {
        out.push(' ');
    }
    out.push_str(&text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(d: &str) -> Path {
        Path::parse(d).expect("valid path data")
    }

    #[test]
    fn parses_implicit_separators() {
        let path = parse("M0 0l-3.943-83.327");
        assert_eq!(path.commands.len(), 2);
        assert_eq!(path.commands[1].values, vec![-3.943, -83.327]);

        let path = parse("M0.5.3");
        assert_eq!(path.commands[0].values, vec![0.5, 0.3]);
        let path = parse("M1e-2,2");
        assert_eq!(path.commands[0].values, vec![0.01, 2.0]);
    }

    #[test]
    fn parses_compact_arc_flags() {
        // `a40 40 0 0140 0` is rx ry rot=0 large=0 sweep=1 x=40 y=0.
        let path = parse("M0 0a40 40 0 0140 0");
        assert_eq!(path.commands[1].values, vec![40.0, 40.0, 0.0, 0.0, 1.0, 40.0, 0.0]);
        // `a10 10 0 1010 10` is rx ry rot=0 large=1 sweep=0 x=10 y=10.
        let path = parse("M0 0a10 10 0 1010 10");
        assert_eq!(path.commands[1].values, vec![10.0, 10.0, 0.0, 1.0, 0.0, 10.0, 10.0]);
    }

    #[test]
    fn rejects_malformed_data() {
        assert!(Path::parse("L10 10").is_none(), "data before moveto");
        assert!(Path::parse("M10").is_none(), "missing argument");
        assert!(Path::parse("M0 0 Z 5").is_none(), "arguments after close");
        assert!(Path::parse("M0 0 a1 1 0 2 0 1 1").is_none(), "flag must be 0 or 1");
        assert!(Path::parse("M0 0 x").is_none(), "unknown command");
        assert!(Path::parse("M L10 10").is_none(), "command without arguments");
        assert!(Path::parse("M0 0 L").is_none(), "trailing command without arguments");
        assert!(Path::parse("M0 0 L1 1 Z Z").is_some(), "close takes no arguments");
        assert!(Path::parse("M 0 0 L 1 1").is_some());
        assert!(Path::parse("").is_some());
    }

    #[test]
    fn segments_expand_implicit_and_relative_forms() {
        let segs = parse("M10 10 20 20 l5 5 h10 v-5").segments().unwrap();
        assert_eq!(
            segs,
            vec![
                Segment::Move { x: 10.0, y: 10.0 },
                Segment::Line { x: 20.0, y: 20.0 },
                Segment::Line { x: 25.0, y: 25.0 },
                Segment::Line { x: 35.0, y: 25.0 },
                Segment::Line { x: 35.0, y: 20.0 },
            ]
        );
    }

    #[test]
    fn segments_resolve_smooth_reflection() {
        let segs = parse("M0 0 C 10 0 20 10 30 10 S 50 20 60 20").segments().unwrap();
        match segs[2] {
            Segment::Cubic { x1, y1, .. } => {
                assert!((x1 - 40.0).abs() < 1e-9);
                assert!((y1 - 10.0).abs() < 1e-9);
            }
            other => panic!("expected cubic, got {:?}", other),
        }
    }

    #[test]
    fn compact_arc_flags_round_trip() {
        let d = "M0 0a40 40 0 0140 0a10 10 0 1010 10";
        let out = Path::parse(d).unwrap().to_string();
        assert_eq!(Path::parse(&out).unwrap().commands, Path::parse(d).unwrap().commands);
        assert!(out.len() <= d.len() + 2, "{}", out);
    }

    #[test]
    fn emit_round_trips_geometry() {
        let d = "M10 10L20 20C30 30 40 40 50 50Q60 60 70 70ZM5 5A3 3 0 0 1 9 9";
        let segs = parse(d).segments().unwrap();
        let out = Path::emit(&segs, None);
        assert_eq!(parse(&out).segments().unwrap(), segs);
    }

    #[test]
    fn emit_snaps_without_drift() {
        // Relative offsets are computed from snapped positions, so the absolute
        // position after many steps stays on the grid.
        let mut segs = vec![Segment::Move { x: 0.0, y: 0.0 }];
        segs.extend((1..=100).map(|i| Segment::Line { x: 0.1234 * i as f64, y: 0.0 }));
        let out = Path::emit(&segs, Some(2));
        let back = parse(&out).segments().unwrap();
        let last = match back.last().unwrap() {
            Segment::Line { x, .. } => *x,
            _ => panic!(),
        };
        assert!((last - 12.34).abs() < 1e-6, "drifted to {} in {}", last, out);
    }
}
