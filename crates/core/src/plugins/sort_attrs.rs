use crate::plugin::SingleElementPluginTrait;
use std::collections::BTreeMap;
use std::error::Error;
use xmltree::Element;

pub struct SortAttrsPlugin;

impl SingleElementPluginTrait for SortAttrsPlugin {
    fn process(&self, element: &mut Element) -> Result<Element, Box<dyn Error>> {
        let mut element_clone = element.clone();

        // Sort attributes alphabetically
        let sorted_attrs: BTreeMap<_, _> = element_clone.attributes.clone().into_iter().collect();

        element_clone.attributes.clear();
        for (k, v) in sorted_attrs {
            element_clone.attributes.insert(k, v);
        }

        Ok(element_clone)
    }
}
