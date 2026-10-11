use crate::svg::Svg;
use std::error::Error;
use xmltree::Element;

pub trait WholeSVGPluginTrait {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>>;
}

pub trait SingleElementPluginTrait {
    fn process(&self, element: &mut Element) -> Result<Element, Box<dyn Error>>;
}
