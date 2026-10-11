use crate::helpers::element_to_string;
use crate::plugin::SingleElementPluginTrait;
use crate::plugin::WholeSVGPluginTrait;
use crate::svg::Svg;
use std::error::Error;
use xmltree::Element;

fn is_container_element(element: &Element) -> bool {
    if element.name == "g"
        || element.name == "a"
        || element.name == "clipPath"
        || element.name == "defs"
        || element.name == "marker"
        || element.name == "mask"
        || element.name == "pattern"
        || element.name == "switch"
        || element.name == "symbol"
        || element.name == "unknown"
    {
        return true;
    }

    return false;
}

pub struct Arbiter {
    single_element_plugins: Vec<Box<dyn SingleElementPluginTrait>>,
    whole_svg_plugins: Vec<(Box<dyn WholeSVGPluginTrait>, bool)>,
}

impl Arbiter {
    pub fn new() -> Self {
        Arbiter {
            single_element_plugins: Vec::new(),
            whole_svg_plugins: Vec::new(),
        }
    }

    pub fn add_single_element_plugin(&mut self, plugin: Box<dyn SingleElementPluginTrait>) {
        self.single_element_plugins.push(plugin);
    }

    pub fn add_whole_svg_plugin(&mut self, plugin: Box<dyn WholeSVGPluginTrait>) {
        self.whole_svg_plugins.push((plugin, false));
    }

    pub fn add_forced_whole_svg_plugin(&mut self, plugin: Box<dyn WholeSVGPluginTrait>) {
        self.whole_svg_plugins.push((plugin, true));
    }

    pub fn process_child_elements(
        element: &mut Element,
        single_plugin: &Box<dyn SingleElementPluginTrait>,
    ) -> Result<(), Box<dyn Error>> {
        for node in &mut element.children {
            // Text, comments and whitespace between elements are not processed.
            let Some(element) = node.as_mut_element() else {
                continue;
            };
            if is_container_element(element) {
                let _ = Arbiter::process_child_elements(element, single_plugin);
            }
            let element_plugin_result = single_plugin.process(element)?;
            if element_to_string(&element_plugin_result).unwrap().len()
                < element_to_string(element).unwrap().len()
            {
                *element = element_plugin_result;
            }
        }

        Ok(())
    }

    pub fn process(&self, svg: &Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();

        for single_plugin in &self.single_element_plugins {
            let root_plugin_result = single_plugin.process(&mut svg_clone.root)?;

            if element_to_string(&root_plugin_result).unwrap().len()
                < element_to_string(&svg_clone.root).unwrap().len()
            {
                svg_clone.root = root_plugin_result;
            }

            let _ = Arbiter::process_child_elements(&mut svg_clone.root, single_plugin);
        }

        for (whole_svg_plugin, forced) in &self.whole_svg_plugins {
            let svg_output = whole_svg_plugin.process(&mut svg_clone)?;
            if *forced || svg_output.to_string().len() < svg_clone.to_string().len() {
                svg_clone = svg_output;
            }
        }

        Ok(svg_clone)
    }
}
