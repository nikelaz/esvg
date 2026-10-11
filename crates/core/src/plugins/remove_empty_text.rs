use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
use std::error::Error;
use xmltree::{Element, XMLNode};

/// Attributes that position text. An element carrying one of these affects the
/// layout even without content.
const POSITIONING_ATTRIBUTES: &[&str] = &["x", "y", "dx", "dy", "rotate"];

/// Removes `<text>` and `<tspan>` elements that cannot draw anything.
///
/// - A `<text>` with only whitespace and no child elements draws nothing.
/// - A `<tspan>` is kept whenever it has text (even a single space, which
///   separates words), has child elements, positions text, or has an id.
pub struct RemoveEmptyTextPlugin;

impl RemoveEmptyTextPlugin {
    fn process_element(element: &mut Element) {
        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::process_element(child_element);
            }
        }

        element.children.retain(|node| match node.as_element() {
            Some(el) if el.name == "text" || el.name == "tspan" => Self::should_keep(el),
            _ => true,
        });
    }

    fn should_keep(element: &Element) -> bool {
        // Respect xml:space="preserve" and anything that other markup refers to.
        if element.attributes.get("xml:space").map(String::as_str) == Some("preserve") {
            return true;
        }
        if element.attributes.contains_key("id") {
            return true;
        }
        // Children that were not removed are content.
        if element.children.iter().any(|c| c.as_element().is_some()) {
            return true;
        }

        let text = Self::text_content(element);

        if element.name == "tspan" {
            if POSITIONING_ATTRIBUTES
                .iter()
                .any(|name| element.attributes.contains_key(*name))
            {
                return true;
            }
            // Keep the exact text: a space between two tspans is significant.
            return !text.is_empty();
        }

        // A text element's own whitespace collapses away when nothing is drawn.
        !text.trim().is_empty()
    }

    fn text_content(element: &Element) -> String {
        element
            .children
            .iter()
            .filter_map(|node| match node {
                XMLNode::Text(t) | XMLNode::CData(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }
}

impl WholeSVGPluginTrait for RemoveEmptyTextPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        Self::process_element(&mut svg_clone.root);
        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        RemoveEmptyTextPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn keeps_space_between_tspans() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><text><tspan>Hello</tspan><tspan> </tspan><tspan>World</tspan></text></svg>"##);
        assert_eq!(out.matches("<tspan").count(), 3, "{}", out);
    }

    #[test]
    fn keeps_positioning_only_tspan() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><text>A<tspan dx="20"/>B</text></svg>"##);
        assert!(out.contains("dx=\"20\""), "{}", out);
    }

    #[test]
    fn removes_blank_text_element() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><text x="1" y="2">   </text></svg>"##);
        assert!(!out.contains("<text"), "{}", out);
    }
}
