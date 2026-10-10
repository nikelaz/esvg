use crate::plugin::WholeSVGPluginTrait;
use crate::svg::Svg;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use xmltree::{Element, XMLNode};

/**
 * Plugin
 *
 * Name: Mangle IDs
 * Author: Nikola Lazarov
 *
 * Description:
 * Renames ids and classes to short generated names.
 *
 * A name is renamed only when every use of it can be rewritten reliably:
 * - `url(#name)` and `href="#name"` references are rewritten.
 * - A name that appears in a stylesheet, in a `style` attribute, in a bare
 *   (non-`#`) position such as `aria-labelledby` or `begin="name.end"`, or as an
 *   external `href` is left alone. So are hex-looking names, which could be
 *   colours such as `#abc`, and names with characters outside `[A-Za-z0-9_-]`.
 * - Generated names never equal an original name or a stylesheet token, so
 *   nothing can collide or start matching a rule.
 */
pub struct MangleIdsPlugin {
    pub prefix: Option<String>,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// Byte ranges of maximal identifier runs in `text`.
fn ident_runs(text: &str) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_ident_char(c) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            runs.push((s, i));
        }
    }
    if let Some(s) = start {
        runs.push((s, text.len()));
    }
    runs
}

fn is_hex_like(name: &str) -> bool {
    matches!(name.len(), 3 | 4 | 6 | 8) && name.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_ident_char)
}

/// Bijective base-52 name: 0 -> "a", 51 -> "Z", 52 -> "aa", and so on.
fn name_for_index(mut n: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut name = Vec::new();
    loop {
        name.push(ALPHABET[n % ALPHABET.len()]);
        n /= ALPHABET.len();
        if n == 0 {
            break;
        }
        n -= 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap()
}

impl MangleIdsPlugin {
    /// Collects every id and class, and the names that must not be renamed.
    fn collect(element: &Element, ids: &mut BTreeSet<String>, classes: &mut BTreeSet<String>, excluded: &mut HashSet<String>) {
        for (name, value) in &element.attributes {
            match name.as_str() {
                "id" => {
                    ids.insert(value.clone());
                }
                "class" => {
                    classes.extend(value.split_whitespace().map(str::to_string));
                }
                "style" => Self::exclude_all_tokens(value, excluded),
                "href" | "xlink:href" => {
                    let local = value.strip_prefix('#').filter(|r| is_plain_name(r));
                    if local.is_none() {
                        Self::exclude_all_tokens(value, excluded);
                    }
                }
                _ => {
                    // A name used without `#` is not a reference this plugin can rewrite.
                    for (start, end) in ident_runs(value) {
                        if !value[..start].ends_with('#') {
                            excluded.insert(value[start..end].to_string());
                        }
                    }
                }
            }
        }

        if element.name == "style" {
            for node in &element.children {
                if let XMLNode::Text(text) | XMLNode::CData(text) = node {
                    Self::exclude_all_tokens(text, excluded);
                }
            }
        }

        for child in element.children.iter().filter_map(|c| c.as_element()) {
            Self::collect(child, ids, classes, excluded);
        }
    }

    fn exclude_all_tokens(text: &str, excluded: &mut HashSet<String>) {
        for (start, end) in ident_runs(text) {
            excluded.insert(text[start..end].to_string());
        }
    }

    /// Replaces each `#name` reference in `value` that has a new name.
    fn rewrite_references(value: &str, map: &HashMap<String, String>) -> String {
        let mut out = String::with_capacity(value.len());
        let mut last = 0;
        for (start, end) in ident_runs(value) {
            if let Some(new_name) = map.get(&value[start..end]) {
                if value[..start].ends_with('#') {
                    out.push_str(&value[last..start]);
                    out.push_str(new_name);
                    last = end;
                }
            }
        }
        out.push_str(&value[last..]);
        out
    }

    fn rewrite(element: &mut Element, map: &HashMap<String, String>) {
        for (name, value) in element.attributes.iter_mut() {
            match name.as_str() {
                "id" => {
                    if let Some(new_name) = map.get(value.as_str()) {
                        *value = new_name.clone();
                    }
                }
                "class" => {
                    let renamed: Vec<String> = value
                        .split_whitespace()
                        .map(|c| map.get(c).cloned().unwrap_or_else(|| c.to_string()))
                        .collect();
                    *value = renamed.join(" ");
                }
                _ => *value = Self::rewrite_references(value, map),
            }
        }

        for child in element.children.iter_mut() {
            if let Some(child_element) = child.as_mut_element() {
                Self::rewrite(child_element, map);
            }
        }
    }
}

impl WholeSVGPluginTrait for MangleIdsPlugin {
    fn process(&self, svg: &mut Svg) -> Result<Svg, Box<dyn Error>> {
        let mut svg_clone = svg.clone();

        let mut ids = BTreeSet::new();
        let mut classes = BTreeSet::new();
        let mut excluded = HashSet::new();
        Self::collect(&svg_clone.root, &mut ids, &mut classes, &mut excluded);

        // Generated names must not equal any original name, nor any token used in a
        // stylesheet or bare reference: a class renamed to `a` would start matching
        // a `.a` rule that matched nothing before.
        let originals: HashSet<String> = ids.iter().chain(classes.iter()).cloned().collect();
        let mut next_index = 0usize;
        let mut fresh = || loop {
            let name = format!(
                "{}{}",
                self.prefix.as_deref().unwrap_or(""),
                name_for_index(next_index)
            );
            next_index += 1;
            if !originals.contains(&name) && !excluded.contains(&name) {
                return name;
            }
        };

        let mut map: HashMap<String, String> = HashMap::new();
        for name in ids.iter().chain(classes.iter()) {
            if map.contains_key(name)
                || excluded.contains(name)
                || is_hex_like(name)
                || !is_plain_name(name)
            {
                continue;
            }
            map.insert(name.clone(), fresh());
        }

        Self::rewrite(&mut svg_clone.root, &map);
        Ok(svg_clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        let mut svg = Svg::from_string(&input.to_string()).unwrap();
        MangleIdsPlugin { prefix: None }.process(&mut svg).unwrap().to_string()
    }

    #[test]
    fn names_are_bijective() {
        assert_eq!(name_for_index(0), "a");
        assert_eq!(name_for_index(51), "Z");
        assert_eq!(name_for_index(52), "aa");
        assert_eq!(name_for_index(53), "ab");
    }

    #[test]
    fn rewrites_url_and_href_references() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><defs><linearGradient id="grad"/></defs><rect fill="url('#grad')"/><use xlink:href="#grad"/></svg>"##);
        assert!(!out.contains("grad"), "{}", out);
    }

    #[test]
    fn same_name_chain_is_not_double_mapped() {
        // A -> a and a -> b would chain if the map were applied repeatedly.
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><linearGradient id="A"/><linearGradient id="a"/><rect fill="url(#A)"/><rect fill="url(#a)"/></svg>"##);
        assert_eq!(out.matches("url(#").count(), 2);
        let refs: Vec<&str> = out.split("url(#").skip(1).map(|s| &s[..s.find(')').unwrap()]).collect();
        assert_ne!(refs[0], refs[1], "{}", out);
    }

    #[test]
    fn names_used_in_stylesheet_are_kept() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><style>#xy{fill:red}</style><rect id="xy"/><rect id="x"/></svg>"##);
        assert!(out.contains("id=\"xy\""));
    }

    #[test]
    fn generated_name_does_not_match_stylesheet_rule() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.a{fill:red}</style><rect class="foo"/></svg>"##);
        assert!(!out.contains("class=\"a\""), "{}", out);
        assert!(!out.contains("class=\"foo\""), "{}", out);
    }

    #[test]
    fn hex_looking_ids_are_kept() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect id="abc" fill="#abc"/></svg>"##);
        assert!(out.contains("id=\"abc\""));
        assert!(out.contains("fill=\"#abc\""));
    }

    #[test]
    fn classes_are_renamed_consistently() {
        let out = run(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect class="cls-1 other"/><rect class="cls-1"/></svg>"##);
        assert!(!out.contains("cls-1"), "{}", out);
    }
}
