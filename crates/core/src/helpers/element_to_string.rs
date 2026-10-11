use std::error::Error;
use xmltree::Element;

pub fn element_to_string(element: &Element) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    element.write(&mut buffer)?;
    let res = String::from_utf8(buffer)?;
    Ok(res)
}
