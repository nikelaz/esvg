//! Strict SVG number scanning and compact number formatting.

/// Scans one SVG number that starts at byte offset `start` of `s`.
///
/// Follows the SVG number grammar: an optional sign, digits with an optional
/// fractional part (`.5` and `5.` are both accepted), and an optional exponent.
/// Returns the value and the offset just past the number, or `None` if no
/// number starts at `start`.
///
/// The scanner stops before a second `.` or a `-`/`+` sign that is not part of
/// an exponent, so `1.5.5` is read as `1.5` followed by `.5`, and `10-20` as
/// `10` followed by `-20`.
pub fn scan_number(s: &str, start: usize) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = start;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let value = s[start..i].parse::<f64>().ok()?;
    Some((value, i))
}

/// Skips whitespace and commas, the separators allowed between SVG numbers.
pub fn skip_separators(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b',') {
        i += 1;
    }
    i
}

/// Parses a whole string as a list of numbers separated by whitespace and/or
/// commas. Returns `None` if anything else appears.
pub fn parse_list(s: &str) -> Option<Vec<f64>> {
    let mut values = Vec::new();
    let mut i = skip_separators(s, 0);
    while i < s.len() {
        let (v, next) = scan_number(s, i)?;
        values.push(v);
        i = skip_separators(s, next);
    }
    Some(values)
}

/// Parses an attribute value that must be exactly one plain number. Values with
/// units (`10px`) or percentages (`50%`) yield `None`.
pub fn parse_plain(s: &str) -> Option<f64> {
    let t = s.trim();
    let (v, end) = scan_number(t, 0)?;
    if end == t.len() {
        Some(v)
    } else {
        None
    }
}

/// Formats a number in the shortest form that round-trips through `f32`.
///
/// Values are stored as `f32` in path data, so numbers are printed with `f32`
/// precision. Very small or very large magnitudes use exponent notation when
/// that is shorter (`0.00001` becomes `1e-5`).
pub fn format_number(value: f64) -> String {
    // Adding 0.0 turns -0.0 into 0.0, which prints without a sign.
    let v = value as f32 + 0.0;
    let plain = format!("{}", v);
    if plain.len() > 5 {
        let exp = format!("{:e}", v);
        if exp.len() < plain.len() {
            return exp;
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_implicit_separators() {
        assert_eq!(scan_number("1.5.5", 0), Some((1.5, 3)));
        assert_eq!(scan_number(".5.5", 0), Some((0.5, 2)));
        assert_eq!(scan_number("10-20", 0), Some((10.0, 2)));
        assert_eq!(scan_number("1e-2x", 0), Some((0.01, 4)));
        assert_eq!(scan_number("1e", 0), Some((1.0, 1)));
        assert_eq!(scan_number("-", 0), None);
        assert_eq!(scan_number("abc", 0), None);
    }

    #[test]
    fn parses_lists_and_plain_values() {
        assert_eq!(parse_list("1, 2 3,4"), Some(vec![1.0, 2.0, 3.0, 4.0]));
        assert_eq!(parse_list("1 px"), None);
        assert_eq!(parse_plain(" 12.5 "), Some(12.5));
        assert_eq!(parse_plain("12px"), None);
        assert_eq!(parse_plain("50%"), None);
    }

    #[test]
    fn formats_compactly() {
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(-3.0), "-3");
        assert_eq!(format_number(0.00001), "1e-5");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
    }
}
