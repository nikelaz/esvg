use crate::plugin::WholeSVGPluginTrait;
use crate::Svg;
/**
 * Plugin
 *
 * Name: Collapse Groups
 * Author: Nikola Lazarov
 *
 * Description:
 * Collapses unnecessary SVG group elements:
 * - Removes empty groups (no children)
 * - Unwraps transparent groups (no meaningful attributes) replacing them with their children
 * - Collapses single-child groups by merging group attributes onto the child element
 *
 */
use std::error::Error;
use xmltree::Element;

/// Properties that are not inherited. Their value on a group applies to the
/// group as a whole, so they cannot be merged onto a child that sets its own.
const NON_INHERITED: &[&str] = &[
    "opacity",
    "display",
    "overflow",
    "clip",
    "clip-path",
    "mask",
    "mask-type",
    "filter",
    "stop-color",
    "stop-opacity",
    "flood-color",
    "flood-opacity",
    "lighting-color",
    "mix-blend-mode",
    "isolation",
    "transform-origin",
    "systemLanguage",
    "requiredFeatures",
    "requiredExtensions",
];

pub struct CollapseGroupsPlugin;

impl CollapseGroupsPlugin {
    fn process_element(element: &mut Element) {
        // First, recurse into children (bottom-up processing)
        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::process_element(child_element);
            }
        }

        // Now process this element's children to collapse groups
        Self::collapse_child_groups(element);
    }

    fn collapse_child_groups(element: &mut Element) {
        // A <switch> renders only its first matching direct child, so its children
        // must stay exactly as they are: unwrapping a group would expose the
        // group's first child alone, and removing an empty one changes the match.
        if element.name == "switch" {
            return;
        }

        let mut new_children = Vec::new();

        for child in element.children.drain(..) {
            match child {
                xmltree::XMLNode::Element(group) if group.name == "g" => {
                    let child_element_count = group
                        .children
                        .iter()
                        .filter(|c| c.as_element().is_some())
                        .count();

                    // Case 1: Empty group — remove entirely
                    if child_element_count == 0 && group.children.is_empty() {
                        continue;
                    }

                    // Case 2: Transparent group (no meaningful attributes) — unwrap children
                    if Self::is_transparent_group(&group) {
                        for grandchild in group.children {
                            new_children.push(grandchild);
                        }
                        continue;
                    }

                    // Case 3: Single-child group — merge attributes onto child
                    if child_element_count == 1 && group.children.len() == 1 {
                        let can_merge = group
                            .children
                            .iter()
                            .find_map(|c| c.as_element())
                            .map_or(false, |child| Self::can_merge(&group, child));
                        if can_merge {
                            let merged = Self::merge_single_child(group);
                            new_children.push(xmltree::XMLNode::Element(merged));
                            continue;
                        }
                    }

                    // Group not collapsible — keep as-is
                    new_children.push(xmltree::XMLNode::Element(group));
                }
                xmltree::XMLNode::Element(defs) if defs.name == "defs" => {
                    let child_element_count = defs
                        .children
                        .iter()
                        .filter(|c| c.as_element().is_some())
                        .count();

                    // Remove empty defs
                    if child_element_count == 0 {
                        continue;
                    }

                    new_children.push(xmltree::XMLNode::Element(defs));
                }
                other => {
                    // Not a group element — keep as-is
                    new_children.push(other);
                }
            }
        }

        element.children = new_children;
    }

    /// A group is "transparent" when it has no attributes at all. Any attribute
    /// can change how its children render (visibility, color, markers, conditional
    /// processing), so none of them may be dropped by unwrapping the group.
    fn is_transparent_group(group: &Element) -> bool {
        group.attributes.is_empty()
    }

    /// True when the group's attributes can be moved onto its only child without
    /// changing the rendering.
    fn can_merge(group: &Element, child: &Element) -> bool {
        // These depend on the group's own coordinate system or contents.
        if ["clip-path", "mask", "filter", "transform-origin"]
            .iter()
            .any(|name| group.attributes.contains_key(*name))
        {
            return false;
        }

        // A clip or mask on the child would be moved into the group's transformed
        // coordinate system, so the group's transform must stay on the group.
        if group.attributes.contains_key("transform")
            && ["clip-path", "mask", "filter"]
                .iter()
                .any(|name| child.attributes.contains_key(*name))
        {
            return false;
        }

        for (name, _) in &group.attributes {
            if child.attributes.contains_key(name) {
                // Both sides define it. Inherited properties resolve in favour of
                // the child, which is correct. Non-inherited ones would be lost, and
                // identity/class/style values would be lost.
                if NON_INHERITED.contains(&name.as_str())
                    || matches!(name.as_str(), "id" | "class" | "style")
                {
                    return false;
                }
            }
        }

        true
    }

    /// Merge a single-child group's attributes onto its child element.
    /// Takes ownership of the group, returns the merged child element.
    /// Callers must check `can_merge` first.
    fn merge_single_child(mut group: Element) -> Element {
        let child_node = match group.children.pop() {
            Some(node) => node,
            None => return group,
        };

        let mut child = match child_node {
            xmltree::XMLNode::Element(el) => el,
            _ => return group,
        };

        // Merge each group attribute onto the child
        for (attr_name, attr_value) in &group.attributes {
            if attr_name == "transform" {
                // Concatenate transforms: group transform first, then child transform
                if let Some(child_transform) = child.attributes.get("transform") {
                    let merged = format!("{} {}", attr_value, child_transform);
                    child.attributes.insert("transform".to_string(), merged);
                } else {
                    child
                        .attributes
                        .insert("transform".to_string(), attr_value.clone());
                }
            } else if child.attributes.contains_key(attr_name) {
                // Child already has this attribute and inherited properties resolve
                // in its favour (checked by `can_merge`)
            } else {
                // Child doesn't have it — inherit from group
                child
                    .attributes
                    .insert(attr_name.clone(), attr_value.clone());
            }
        }

        child
    }
}

impl WholeSVGPluginTrait for CollapseGroupsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        Self::process_element(&mut svg.root);

        Ok(svg.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        CollapseGroupsPlugin.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn keeps_groups_inside_switch() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><switch><g><rect width="1" height="1"/><circle r="1"/></g><rect width="2" height="2"/></switch></svg>"##);
        assert!(out.contains("<switch><g>"), "{}", out);
    }

    #[test]
    fn unwraps_attribute_less_group() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><g><rect width="1" height="1"/><circle r="1"/></g></svg>"##);
        assert!(!out.contains("<g"), "{}", out);
    }
}
