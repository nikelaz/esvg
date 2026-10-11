use crate::helpers::tree::contains_element;
use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
use std::error::Error;
use xmltree::Element;

/// Attributes that never affect rendering.
///
/// `lang` and `xml:lang` are not listed: they choose fonts and language-specific
/// glyphs in text, so removing them can change how text looks.
const UNNECESSARY_ATTRIBUTES: &[&str] = &[
    "desc",
    "title",
    "version",
    "baseProfile",
    "contentStyleType",
    "contentScriptType",
];

/// Removes attributes that have no effect on how the document renders.
///
/// `data-*` attributes are only removed when the document has no <style>
/// element, because a stylesheet can select on them.
pub struct RemoveUnnecessaryAttrsPlugin;

impl RemoveUnnecessaryAttrsPlugin {
    fn strip(element: &mut Element, remove_data: bool) {
        element.attributes.retain(|key, _| {
            !UNNECESSARY_ATTRIBUTES.contains(&key.as_str())
                && !(remove_data && key.starts_with("data-"))
        });
        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::strip(child_element, remove_data);
            }
        }
    }
}

impl WholeSVGPluginTrait for RemoveUnnecessaryAttrsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();
        let remove_data = !contains_element(&svg_clone.root, "style");
        Self::strip(&mut svg_clone.root, remove_data);
        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        RemoveUnnecessaryAttrsPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn removes_metadata_attributes() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg" version="1.1"><rect data-x="1" width="1" height="1"/></svg>"##);
        assert!(!out.contains("version=\"1.1\""), "{}", out);
        assert!(!out.contains("data-x"));
    }

    #[test]
    fn keeps_data_attributes_when_stylesheet_present() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><style>rect[data-k]{fill:red}</style><rect data-k="1"/></svg>"##);
        assert!(out.contains("data-k"));
    }

    #[test]
    fn keeps_language_attributes() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><text xml:lang="ja">x</text></svg>"##);
        assert!(out.contains("xml:lang"));
    }
}
