use std::fmt;
use xmltree::{Element, EmitterConfig, ParserConfig, XMLNode};

#[derive(Debug, Clone)]
pub struct Svg {
    pub root: Element,
}

/// Elements whose text content is rendered, where whitespace is significant.
fn is_text_container(name: &str) -> bool {
    matches!(name, "text" | "tspan" | "textPath")
}

/// Drops whitespace-only text nodes outside text content. They are indentation
/// between elements and affect nothing. Inside text, a space is the gap between
/// words and must be kept.
fn strip_layout_whitespace(element: &mut Element, in_text: bool) {
    let in_text = in_text || is_text_container(&element.name);
    if !in_text {
        element
            .children
            .retain(|node| !matches!(node, XMLNode::Text(t) if t.trim().is_empty()));
    }
    for child in element.children.iter_mut() {
        if let Some(child_element) = child.as_mut_element() {
            strip_layout_whitespace(child_element, in_text);
        }
    }
}

/// Stands in for the colon in prefixed attribute names while parsing.
const PREFIX_MARKER: &str = "__esvg_ns__";

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.')
}

/// Renames one start or end tag's prefixed attributes (`xlink:href`, `xml:space`).
/// Attribute values, the element name and `xmlns` declarations are left as they are.
fn mask_tag_attributes(tag: &str) -> String {
    let chars: Vec<char> = tag.chars().collect();
    let mut out = String::with_capacity(tag.len() + 16);
    let mut quote: Option<char> = None;
    let mut seen_element_name = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
        } else if c == '"' || c == '\'' {
            quote = Some(c);
            out.push(c);
            i += 1;
        } else if is_name_char(c) {
            let start = i;
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            if !seen_element_name {
                seen_element_name = true;
                out.push_str(&token);
                continue;
            }
            let mut next = i;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            let is_attribute = next < chars.len() && chars[next] == '=';
            if is_attribute && token.contains(':') && !token.starts_with("xmlns") {
                out.push_str(&token.replace(':', PREFIX_MARKER));
            } else {
                out.push_str(&token);
            }
        } else {
            out.push(c);
            i += 1;
        }
    }

    out
}

/// Masks prefixed attribute names in every start and end tag of the document.
fn mask_prefixed_attributes(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 64);
    let mut rest = input;

    while let Some(pos) = rest.find('<') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];

        // Comments, CDATA sections, processing instructions and declarations are copied as-is.
        let skip_to = if rest.starts_with("<!--") {
            rest.find("-->").map(|e| e + 3)
        } else if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map(|e| e + 3)
        } else if rest.starts_with("<?") {
            rest.find("?>").map(|e| e + 2)
        } else if rest.starts_with("<!") {
            rest.find('>').map(|e| e + 1)
        } else {
            None
        };
        if let Some(end) = skip_to {
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }

        // A start or end tag. Quotes may contain '>', so find the real end.
        let mut quote: Option<char> = None;
        let mut end = rest.len();
        for (i, c) in rest.char_indices() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => match c {
                    '"' | '\'' => quote = Some(c),
                    '>' => {
                        end = i + 1;
                        break;
                    }
                    _ => {}
                },
            }
        }
        out.push_str(&mask_tag_attributes(&rest[..end]));
        rest = &rest[end..];
    }

    out.push_str(rest);
    out
}

/// Restores the colon in attribute names that were masked before parsing.
fn restore_prefixed_attributes(element: &mut Element) {
    let needs_restore = element.attributes.keys().any(|k| k.contains(PREFIX_MARKER));
    if needs_restore {
        let restored: Vec<(String, String)> = element
            .attributes
            .iter()
            .map(|(k, v)| (k.replace(PREFIX_MARKER, ":"), v.clone()))
            .collect();
        element.attributes.clear();
        for (k, v) in restored {
            element.attributes.insert(k, v);
        }
    }
    for child in element.children.iter_mut() {
        if let Some(child_element) = child.as_mut_element() {
            restore_prefixed_attributes(child_element);
        }
    }
}

impl Svg {
    pub fn from_string(svg_string: &String) -> Result<Self, String> {
        // xmltree keeps only the local part of attribute names, which would turn
        // xlink:href into href and xml:space into space, so the colon is masked
        // during parsing.
        let masked = mask_prefixed_attributes(svg_string);

        // By default xml-rs reports whitespace-only text as a separate event that
        // xmltree discards, which would remove the space between two <tspan>s.
        let config = ParserConfig::new().whitespace_to_characters(true);
        let mut root = Element::parse_with_config(masked.as_bytes(), config)
            .map_err(|e| e.to_string())?;
        restore_prefixed_attributes(&mut root);
        strip_layout_whitespace(&mut root, false);

        Ok(Svg { root })
    }

    pub fn new(root: &Element) -> Self {
        Self { root: root.clone() }
    }

    pub fn to_pretty_string(&self) -> Result<String, String> {
        let mut buffer = Vec::new();
        let config = EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(false);

        self.root
            .write_with_config(&mut buffer, config)
            .map_err(|e| e.to_string())?;

        String::from_utf8(buffer).map_err(|e| e.to_string())
    }
}

impl fmt::Display for Svg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buffer = Vec::new();
        // The XML declaration is optional for SVG and defaults to UTF-8.
        let config = EmitterConfig::new().write_document_declaration(false);

        if let Err(e) = self.root.write_with_config(&mut buffer, config) {
            return write!(f, "<invalid SVG: {}>", e);
        }

        match String::from_utf8(buffer) {
            Ok(s) => f.write_str(&s),
            Err(e) => write!(f, "<invalid UTF-8: {}>", e),
        }
    }
}
