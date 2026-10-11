use roxmltree::{Document, Node};

fn react_component_name(file_stem: &str) -> String {
    let mut name = String::new();
    let mut capitalize = true;
    for character in file_stem.chars() {
        if character.is_ascii_alphanumeric() {
            if name.is_empty() && character.is_ascii_digit() {
                name.push_str("Svg");
            }
            name.push(if capitalize {
                character.to_ascii_uppercase()
            } else {
                character
            });
            capitalize = false;
        } else {
            capitalize = true;
        }
    }
    if name.is_empty() {
        "SvgComponent".to_string()
    } else if name == "React" {
        "ReactComponent".to_string()
    } else {
        name
    }
}

fn jsx_attribute_name(name: &str) -> String {
    match name {
        "class" => return "className".into(),
        "for" => return "htmlFor".into(),
        "tabindex" => return "tabIndex".into(),
        "crossorigin" => return "crossOrigin".into(),
        _ => {}
    }
    if name.starts_with("aria-") || name.starts_with("data-") {
        return name.into();
    }
    let mut output = String::with_capacity(name.len());
    let mut uppercase_next = false;
    for character in name.chars() {
        if matches!(character, '-' | ':') {
            uppercase_next = true;
        } else if uppercase_next {
            output.push(character.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            output.push(character);
        }
    }
    output
}

fn jsx_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn jsx_text(value: &str) -> String {
    format!("{{{}}}", jsx_string(value))
}

fn jsx_style(value: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut quote = None;
    let mut depth = 0usize;
    let mut escaped = false;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if escaped {
            part.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            part.push(character);
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            part.push(character);
            if character == active_quote {
                quote = None;
            }
            continue;
        }
        if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            let mut closed = false;
            while let Some(next) = characters.next() {
                if next == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    closed = true;
                    break;
                }
            }
            if !closed {
                return Err("Unclosed CSS comment in inline style".into());
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                part.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                part.push(character);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                part.push(character);
            }
            ';' if depth == 0 => {
                parts.push(std::mem::take(&mut part));
            }
            _ => part.push(character),
        }
    }
    if quote.is_some() || depth > 0 {
        return Err("Unclosed string or block in inline style".into());
    }
    parts.push(part);
    let mut declarations = Vec::new();
    for declaration in parts {
        if let Some((name, value)) = declaration.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if !name.is_empty() && !value.is_empty() {
                if value
                    .rsplit_once('!')
                    .is_some_and(|(_, priority)| priority.trim().eq_ignore_ascii_case("important"))
                {
                    return Err("React style objects cannot preserve !important. Move this declaration to a stylesheet before exporting.".into());
                }
                let name = if name.starts_with("--") {
                    name.to_string()
                } else if name.starts_with("-ms-") {
                    jsx_attribute_name(&name[1..])
                } else {
                    jsx_attribute_name(name)
                };
                declarations.push(format!("{}: {}", jsx_string(&name), jsx_string(value)));
            }
        }
    }
    Ok(format!("{{{{ {} }}}}", declarations.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::react_component_source;

    #[test]
    fn preserves_attribute_quotes_backslashes_and_entity_text() {
        let source = react_component_source(
            r#"<svg><text data-label="a&quot;b\c" xml:space="preserve">  &amp;copy; {x} &lt;tag&gt;
end  </text></svg>"#,
            "icon",
        )
        .unwrap();
        assert!(source.contains(r#"data-label={"a\"b\\c"}"#));
        assert!(source.contains(r#"xmlSpace={"preserve"}"#));
        assert!(source.contains(r#"{"  &copy; {x} <tag>\nend  "}"#));
    }

    #[test]
    fn converts_styles_without_splitting_urls_or_strings() {
        let source = react_component_source(
            r#"<svg style="stroke-width:2;-webkit-transform:scale(2);-ms-transform:scale(1);--color:red;fill:url('data:image/svg+xml;utf8,test');font-family:'A; B';/*comment*/fill-opacity:.5"/>"#,
            "icon",
        ).unwrap();
        for declaration in [
            r#""strokeWidth": "2""#,
            r#""WebkitTransform": "scale(2)""#,
            r#""msTransform": "scale(1)""#,
            r#""--color": "red""#,
            r#""fill": "url('data:image/svg+xml;utf8,test')""#,
            r#""fontFamily": "'A; B'""#,
            r#""fillOpacity": ".5""#,
        ] {
            assert!(
                source.contains(declaration),
                "Missing {declaration}: {source}"
            );
        }
    }

    #[test]
    fn normalizes_svg_prefixes_and_preserves_foreign_metadata() {
        let source = react_component_source(
            r##"<s:svg xmlns:s="http://www.w3.org/2000/svg" xmlns:l="http://www.w3.org/1999/xlink" xmlns:m="urn:metadata"><s:use l:href="#shape"/><s:metadata><m:RDF m:label="a"/></s:metadata></s:svg>"##,
            "icon",
        ).unwrap();
        assert!(source.contains("<svg "));
        assert!(!source.contains("<s:"));
        assert!(source.contains(r##"<use xlinkHref={"#shape"}"##));
        assert!(source.contains(r#"const SvgExportTag0 = "m:RDF";"#));
        assert!(source.contains(r#"<SvgExportTag0 {...{ "m:label": "a" }}"#));
    }

    #[test]
    fn names_components_safely_and_allows_root_prop_overrides() {
        let source = react_component_source(r#"<svg width="24"/>"#, "React").unwrap();
        assert!(source.contains("function ReactComponent(props)"));
        assert!(source.contains(r#"width={"24"} {...props}"#));
        let source = react_component_source("<svg/>", "123-icon").unwrap();
        assert!(source.contains("function Svg123Icon(props)"));
    }

    #[test]
    fn reports_invalid_or_unrepresentable_input() {
        assert!(react_component_source("<svg>", "icon").is_err());
        assert!(react_component_source("<html/>", "icon").is_err());
        let error =
            react_component_source(r#"<svg style="fill:red ! important"/>"#, "icon").unwrap_err();
        assert!(error.contains("!important"));
    }
}

fn jsx_element(
    node: Node<'_, '_>,
    is_root: bool,
    aliases: &mut Vec<(String, String)>,
) -> Result<String, String> {
    let tag = node.tag_name();
    let qualified_tag = tag
        .namespace()
        .and_then(|uri| node.namespaces().find(|namespace| namespace.uri() == uri))
        .and_then(|namespace| {
            namespace
                .name()
                .map(|prefix| format!("{prefix}:{}", tag.name()))
        })
        .unwrap_or_else(|| tag.name().to_string());
    let intrinsic_tag = match tag.namespace() {
        Some("http://www.w3.org/2000/svg" | "http://www.w3.org/1999/xhtml") => tag.name(),
        _ => &qualified_tag,
    };
    let tag_name = if intrinsic_tag.contains(':')
        || !intrinsic_tag.starts_with(|character: char| character.is_ascii_lowercase())
    {
        if let Some((alias, _)) = aliases.iter().find(|(_, name)| name == intrinsic_tag) {
            alias.clone()
        } else {
            let alias = format!("SvgExportTag{}", aliases.len());
            aliases.push((alias.clone(), intrinsic_tag.to_string()));
            alias
        }
    } else {
        intrinsic_tag.to_string()
    };
    let mut output = format!("<{tag_name}");
    if is_root {
        for namespace in node.namespaces() {
            match namespace.name() {
                None => output.push_str(&format!(" xmlns={{{}}}", jsx_string(namespace.uri()))),
                Some("xlink") => {
                    output.push_str(&format!(" xmlnsXlink={{{}}}", jsx_string(namespace.uri())))
                }
                Some(prefix) => output.push_str(&format!(
                    " {{...{{ {}: {} }}}}",
                    jsx_string(&format!("xmlns:{prefix}")),
                    jsx_string(namespace.uri())
                )),
            }
        }
    }
    for attribute in node.attributes() {
        let name = attribute
            .namespace()
            .and_then(|uri| node.namespaces().find(|namespace| namespace.uri() == uri))
            .and_then(|namespace| {
                namespace
                    .name()
                    .map(|prefix| format!("{prefix}:{}", attribute.name()))
            })
            .unwrap_or_else(|| attribute.name().to_string());
        let name = match attribute.namespace() {
            Some("http://www.w3.org/1999/xlink") => format!("xlink:{}", attribute.name()),
            Some("http://www.w3.org/XML/1998/namespace") => format!("xml:{}", attribute.name()),
            _ => name,
        };
        if attribute.namespace().is_some()
            && !matches!(
                attribute.namespace(),
                Some("http://www.w3.org/1999/xlink" | "http://www.w3.org/XML/1998/namespace")
            )
        {
            output.push_str(&format!(
                " {{...{{ {}: {} }}}}",
                jsx_string(&name),
                jsx_string(attribute.value())
            ));
            continue;
        }
        let name = jsx_attribute_name(&name);
        let value = attribute.value();
        if name == "style" {
            output.push_str(&format!(" style={}", jsx_style(value)?));
        } else {
            output.push_str(&format!(" {name}={{{}}}", jsx_string(value)));
        }
    }
    if is_root {
        output.push_str(" {...props}");
    }
    let children: Vec<_> = node.children().collect();
    if children.is_empty() {
        output.push_str(" />");
        return Ok(output);
    }
    output.push('>');
    for child in children {
        match child.node_type() {
            roxmltree::NodeType::Element => output.push_str(&jsx_element(child, false, aliases)?),
            roxmltree::NodeType::Text => {
                if let Some(text) = child.text() {
                    output.push_str(&jsx_text(text));
                }
            }
            _ => {}
        }
    }
    output.push_str(&format!("</{tag_name}>"));
    Ok(output)
}

pub fn react_component_source(svg: &str, file_stem: &str) -> Result<String, String> {
    let component_name = react_component_name(file_stem);
    let document = Document::parse(svg).map_err(|error| error.to_string())?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err("React export requires an SVG root element".into());
    }
    let mut aliases = Vec::new();
    let jsx = jsx_element(root, true, &mut aliases)?;
    let aliases: String = aliases
        .into_iter()
        .map(|(alias, name)| format!("  const {alias} = {};\n", jsx_string(&name)))
        .collect();
    Ok(format!("import React from \"react\";\n\nexport default function {component_name}(props) {{\n{aliases}  return (\n    {jsx}\n  );\n}}\n"))
}
