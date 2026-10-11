use crate::helpers::tree::contains_element;
use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
/**
* Plugin
*
* Name: CSS to Attributes
* Author: Nikola Lazarov
*
* Description:
* Moves presentation properties from inline `style` attributes to plain
* attributes, which are shorter.
*
* Only done when the document has no <style> element. A presentation attribute
* loses to any stylesheet rule, while an inline style beats it, so moving a
* declaration out of an inline style could change the rendering.
*
* `transform` is not moved: its CSS syntax (units, `transform-box`) differs
* from the SVG attribute grammar. A style attribute that does not parse is left
* untouched.
*
* Todo:
* [x] Convert inline styles to attributes
* [ ] Convert CSS blocks to attributes
*
*/
use std::error::Error;
use xmltree::Element;

const PRESENTATION_ATTRIBUTES: &[&str] = &[
    "fill",
    "fill-rule",
    "fill-opacity",
    "font-size",
    "font-style",
    "font-weight",
    "font-family",
    "filter",
    "clip-path",
    "clip-rule",
    "display",
    "opacity",
    "overflow",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "text-decoration",
];

/// One `name: value` declaration from an inline style.
struct Declaration {
    name: String,
    value: String,
    important: bool,
}

/// Splits a declaration block on `;`, ignoring semicolons inside quotes or parentheses.
fn split_declarations(style: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in style.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => depth -= 1,
                ';' if depth == 0 => {
                    parts.push(&style[start..i]);
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    parts.push(&style[start..]);
    parts
}

/// Parses a whole style attribute. Returns `None` if any part is malformed, so
/// that nothing is changed in that case.
fn parse_declarations(style: &str) -> Option<Vec<Declaration>> {
    let mut declarations = Vec::new();
    for part in split_declarations(style) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let colon = part.find(':')?;
        let name = part[..colon].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        let mut value = part[colon + 1..].trim();
        let important = match value.len().checked_sub("!important".len()) {
            Some(cut) if value[cut..].eq_ignore_ascii_case("!important") => {
                value = value[..cut].trim_end();
                true
            }
            _ => false,
        };
        if value.is_empty() {
            return None;
        }
        declarations.push(Declaration {
            name: name.to_string(),
            value: value.to_string(),
            important,
        });
    }
    Some(declarations)
}

pub struct CssToAttributesPlugin;

impl CssToAttributesPlugin {
    fn traverse_and_apply_styles(element: &mut Element) {
        if element.attributes.contains_key("style") {
            Self::apply_inline_style_as_attr(element);
        }

        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::traverse_and_apply_styles(child_element);
            }
        }
    }

    fn apply_inline_style_as_attr(element: &mut Element) {
        let style = match element.attributes.get("style") {
            Some(s) => s.clone(),
            None => return,
        };
        let declarations = match parse_declarations(&style) {
            Some(d) => d,
            None => return,
        };

        // A shorthand left in the style would override a longhand moved to an
        // attribute, whatever their order was. `font` sets every font-* property
        // and `all` sets everything.
        if declarations.iter().any(|d| d.name == "all") {
            return;
        }
        let has_font_shorthand = declarations.iter().any(|d| d.name == "font");

        let mut remaining: Vec<Declaration> = Vec::new();
        for declaration in declarations {
            let covered_by_shorthand = has_font_shorthand && declaration.name.starts_with("font-");
            if PRESENTATION_ATTRIBUTES.contains(&declaration.name.as_str()) && !covered_by_shorthand {
                // The inline style wins over presentation attributes, so the moved
                // value replaces any attribute with the same name.
                element
                    .attributes
                    .insert(declaration.name, declaration.value);
            } else {
                remaining.push(declaration);
            }
        }

        if remaining.is_empty() {
            element.attributes.remove("style");
        } else {
            let style = remaining
                .iter()
                .map(|d| {
                    let important = if d.important { " !important" } else { "" };
                    format!("{}:{}{}", d.name, d.value, important)
                })
                .collect::<Vec<_>>()
                .join(";");
            element.attributes.insert("style".to_string(), style);
        }
    }
}

impl WholeSVGPluginTrait for CssToAttributesPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        if !contains_element(&svg_clone.root, "style") {
            CssToAttributesPlugin::traverse_and_apply_styles(&mut svg_clone.root);
        }

        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        CssToAttributesPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn moves_presentation_properties() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:#f00;stroke:none" d="M0 0"/></svg>"##);
        assert!(out.contains("fill=\"#f00\""));
        assert!(out.contains("stroke=\"none\""));
        assert!(!out.contains("style"));
    }

    #[test]
    fn keeps_transform_in_style() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect style="transform: translate(50px, 20px); fill: red" x="1"/></svg>"##);
        assert!(out.contains("transform:translate(50px, 20px)"));
        assert!(out.contains("fill=\"red\""));
    }

    #[test]
    fn malformed_style_is_untouched() {
        let input = r##"<svg xmlns="http://www.w3.org/2000/svg"><path style="fill red; x" d="M0 0"/></svg>"##;
        assert!(run(input).contains("style=\"fill red; x\""));
    }

    #[test]
    fn keeps_longhand_next_to_font_shorthand() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><text style="font:12px serif;font-size:20px;fill:red">x</text></svg>"##);
        assert!(out.contains("style=\"font:12px serif;font-size:20px\""), "{}", out);
        assert!(out.contains("fill=\"red\""), "{}", out);
    }

    #[test]
    fn skipped_when_stylesheet_present() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.a{fill:blue}</style><path style="fill:red" d="M0 0"/></svg>"##);
        assert!(out.contains("style=\"fill:red\""));
    }

    #[test]
    fn keeps_important_flag_on_remaining_declarations() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><path style="fill:red !important;font:x y" d="M0 0"/></svg>"##);
        assert!(out.contains("fill=\"red\""));
        assert!(out.contains("style=\"font:x y\""));
    }
}
