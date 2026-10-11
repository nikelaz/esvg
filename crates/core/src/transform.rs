use crate::helpers::number::{parse_list, skip_separators};

/// A 2D affine transform `[a c e; b d f; 0 0 1]`, applied as
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Returns `self * other`: `other` is applied to a point first, then `self`.
    pub fn then_outer(self, other: Affine) -> Affine {
        Affine {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// True when the transform only scales, flips and translates each axis.
    pub fn is_axis_aligned(&self) -> bool {
        self.b.abs() < 1e-12 && self.c.abs() < 1e-12
    }

    pub fn is_translation_only(&self) -> bool {
        self.is_axis_aligned() && (self.a - 1.0).abs() < 1e-12 && (self.d - 1.0).abs() < 1e-12
    }
}

/// Parses an SVG `transform` attribute into a single matrix.
///
/// The list is composed left to right, so `translate(10) scale(2)` maps a point
/// by the scale first. Returns `None` if the value contains anything that is
/// not a supported transform function, or if a function has the wrong number
/// of arguments. A transform the parser cannot read must never be partly
/// applied.
pub fn parse_transform(value: &str) -> Option<Affine> {
    let mut result = Affine::IDENTITY;
    let mut i = skip_separators(value, 0);

    while i < value.len() {
        let open = i + value[i..].find('(')?;
        let name = value[i..open].trim();
        let close = open + value[open..].find(')')?;
        let args = parse_list(&value[open + 1..close])?;
        let op = build_op(name, &args)?;
        result = result.then_outer(op);
        i = skip_separators(value, close + 1);
    }

    Some(result)
}

fn build_op(name: &str, args: &[f64]) -> Option<Affine> {
    match (name, args.len()) {
        ("translate", 1) => Some(translate(args[0], 0.0)),
        ("translate", 2) => Some(translate(args[0], args[1])),
        ("scale", 1) => Some(scale(args[0], args[0])),
        ("scale", 2) => Some(scale(args[0], args[1])),
        ("rotate", 1) => Some(rotate(args[0])),
        ("rotate", 3) => Some(
            translate(args[1], args[2])
                .then_outer(rotate(args[0]))
                .then_outer(translate(-args[1], -args[2])),
        ),
        ("skewX", 1) => Some(Affine {
            c: args[0].to_radians().tan(),
            ..Affine::IDENTITY
        }),
        ("skewY", 1) => Some(Affine {
            b: args[0].to_radians().tan(),
            ..Affine::IDENTITY
        }),
        ("matrix", 6) => Some(Affine {
            a: args[0],
            b: args[1],
            c: args[2],
            d: args[3],
            e: args[4],
            f: args[5],
        }),
        _ => None,
    }
}

fn translate(x: f64, y: f64) -> Affine {
    Affine {
        e: x,
        f: y,
        ..Affine::IDENTITY
    }
}

fn scale(x: f64, y: f64) -> Affine {
    Affine {
        a: x,
        d: y,
        ..Affine::IDENTITY
    }
}

fn rotate(degrees: f64) -> Affine {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Affine {
        a: cos,
        b: sin,
        c: -sin,
        d: cos,
        ..Affine::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn composes_left_to_right() {
        // translate(10 0) scale(2): the point (1, 0) is scaled first, then moved.
        let m = parse_transform("translate(10 0) scale(2)").unwrap();
        assert!(close(m.apply(1.0, 0.0), (12.0, 0.0)));
        let m = parse_transform("scale(2) translate(10 0)").unwrap();
        assert!(close(m.apply(1.0, 0.0), (22.0, 0.0)));
    }

    #[test]
    fn parses_all_supported_functions() {
        let m = parse_transform("rotate(90 10 10)").unwrap();
        assert!(close(m.apply(20.0, 10.0), (10.0, 20.0)));
        assert!(parse_transform("matrix(1,0,0,1,5,6)").unwrap().is_translation_only());
        assert!(parse_transform("matrix(1 0 0 1 5 6)").unwrap().e == 5.0);
        assert!(parse_transform("translate(3)").is_some());
    }

    #[test]
    fn rejects_unknown_or_malformed() {
        assert!(parse_transform("translate(10px, 5px)").is_none());
        assert!(parse_transform("translate(1 2 3)").is_none());
        assert!(parse_transform("foo(1)").is_none());
        assert!(parse_transform("translate(1").is_none());
        assert!(parse_transform("").is_some());
    }
}
