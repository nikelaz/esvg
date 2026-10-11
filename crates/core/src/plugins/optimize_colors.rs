// Plugin
//
// Name: Optimize Colors
// Author: Nikola Lazarov
//
// Description:
// Optimizes color values to their most efficient representation
//
// Todo:
// [x] - Optimize hex colors
// [ ] - Support other color systems/representations

use crate::plugin::SingleElementPluginTrait;
use std::error::Error;
use xmltree::Element;

const COLOR_ATTRIBUTES: [&str; 4] = ["fill", "stroke", "stop-color", "color"];

// Replacements must keep the exact colour. CSS `green` is #008000, not #0f0.
static STATIC_REPLACEMENTS: &[(&str, &str)] = &[
    ("black", "#000"),
    ("white", "#fff"),
    ("blue", "#00f"),
    ("cyan", "#0ff"),
    ("magenta", "#f0f"),
    ("yellow", "#ff0"),
    ("#ff0000", "red"),
    ("#008000", "green"),
];

pub struct OptimizeColorsPlugin;

impl OptimizeColorsPlugin {
    fn normalize_hex_color(color: &str) -> Option<String> {
        if !color.starts_with('#') {
            return None;
        }

        match color.len() {
            4 => {
                // #abc -> #aabbcc
                let chars: Vec<char> = color.chars().collect();
                Some(format!(
                    "#{}{}{}{}{}{}",
                    chars[1], chars[1], chars[2], chars[2], chars[3], chars[3]
                ))
            }
            7 => Some(color.to_string()), // already full hex
            _ => None,                    // unsupported format
        }
    }

    fn optimize_hex_color(color: &str) -> Option<String> {
        if !color.starts_with('#') || color.len() != 7 {
            return None;
        }

        let chars: Vec<char> = color.chars().collect();

        if chars[1] == chars[2] && chars[3] == chars[4] && chars[5] == chars[6] {
            Some(format!("#{}{}{}", chars[1], chars[3], chars[5]))
        } else {
            Some(color.to_string())
        }
    }

    fn optimize_color_value(color_value: &str) -> String {
        let trimmed = color_value.trim();

        // Paint servers and keywords are not colours to rewrite. url() ids are
        // case-sensitive, so the value must not be lowercased.
        if trimmed.contains("url(") || !trimmed.starts_with('#') && !Self::is_named_color(trimmed) {
            return color_value.to_string();
        }

        let color_lower = trimmed.to_ascii_lowercase();
        let color_normalized =
            Self::normalize_hex_color(&color_lower).unwrap_or(color_lower.to_string());

        for &(key, value) in STATIC_REPLACEMENTS {
            if key == color_normalized {
                return value.to_string();
            }
        }

        Self::optimize_hex_color(&color_normalized).unwrap_or(color_lower)
    }

    /// Named colours are case-insensitive keywords in CSS.
    fn is_named_color(value: &str) -> bool {
        value.chars().all(|c| c.is_ascii_alphabetic())
    }

    fn optimize_color_attributes(element: &mut Element) {
        for attr_name in COLOR_ATTRIBUTES.iter() {
            if let Some(color_value) = element.attributes.get_mut(*attr_name) {
                let simplified = Self::optimize_color_value(color_value);
                *color_value = simplified.to_string();
            }
        }
    }
}

impl SingleElementPluginTrait for OptimizeColorsPlugin {
    fn process(&self, element: &mut Element) -> Result<Element, Box<dyn Error>> {
        let mut element_clone = element.clone();

        Self::optimize_color_attributes(&mut element_clone);

        Ok(element_clone)
    }
}
