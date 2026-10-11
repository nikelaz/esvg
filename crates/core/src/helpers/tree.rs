//! Small queries over the element tree shared by the plugins.

use crate::helpers::number::parse_plain;
use xmltree::Element;

/// True when `element` or any element below it has the given name.
pub fn contains_element(element: &Element, name: &str) -> bool {
    element.name == name
        || element
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .any(|c| contains_element(c, name))
}

/// True when the document can draw markers. Marker properties are inherited and
/// can be set from many places, but they draw nothing without a <marker> element.
pub fn has_markers(root: &Element) -> bool {
    contains_element(root, "marker")
}

/// Reads an optional numeric attribute. A missing attribute yields `default`.
/// A present attribute that is not a plain number yields `None`, because the
/// real value (for example a percentage) is unknown.
pub fn num_attr(element: &Element, name: &str, default: f64) -> Option<f64> {
    match element.attributes.get(name) {
        None => Some(default),
        Some(value) => parse_plain(value),
    }
}

/// Elements whose children are never rendered in place: they are templates or
/// resources used from elsewhere, in the coordinate system of the user.
pub fn is_non_rendered(name: &str) -> bool {
    matches!(
        name,
        "defs"
            | "clipPath"
            | "mask"
            | "marker"
            | "pattern"
            | "symbol"
            | "linearGradient"
            | "radialGradient"
            | "filter"
            | "style"
            | "script"
            | "title"
            | "desc"
            | "metadata"
    )
}
