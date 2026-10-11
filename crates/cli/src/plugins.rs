//! Maps the optimizer's plugins to the ids the command line accepts, and turns
//! the user's on/off choices into a `PluginConfig`.

use esvg_rs::PluginConfig;
use std::str::FromStr;

/// Declares `Plugin` from one table, so each plugin's id and `PluginConfig`
/// field are written down exactly once.
macro_rules! plugins {
    ($($variant:ident => $id:literal, $field:ident;)*) => {
        /// One of the optimizer's plugins, in the same order as `esvg_rs::PLUGIN_NAMES`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Plugin {
            $($variant,)*
        }

        impl Plugin {
            pub const ALL: [Plugin; [$($id),*].len()] = [$(Plugin::$variant),*];

            /// The id used on the command line and in JSON output.
            pub fn id(self) -> &'static str {
                match self {
                    $(Plugin::$variant => $id,)*
                }
            }

            fn flag_mut(self, config: &mut PluginConfig) -> &mut bool {
                match self {
                    $(Plugin::$variant => &mut config.$field,)*
                }
            }

            fn flag(self, config: &PluginConfig) -> &bool {
                match self {
                    $(Plugin::$variant => &config.$field,)*
                }
            }
        }
    };
}

plugins! {
    ApplyTransforms => "apply-transforms", apply_transforms;
    CollapseGroups => "collapse-groups", collapse_groups;
    CombinePaths => "combine-paths", combine_paths;
    CssToAttributes => "css-to-attributes", css_to_attributes;
    MangleIds => "mangle-ids", mangle_ids;
    NumberPrecision => "number-precision", number_precision;
    OptimizeColors => "optimize-colors", optimize_colors;
    RemoveEmptyText => "remove-empty-text", remove_empty_text;
    RemoveUnnecessaryAttrs => "remove-unnecessary-attrs", remove_unnecessary_attrs;
    RemoveUnnecessaryClipPaths => "remove-unnecessary-clip-paths", remove_unnecessary_clip_paths;
    ShapeToPath => "shape-to-path", shape_to_path;
    SimplifyPaths => "simplify-paths", simplify_paths;
    SortAttrs => "sort-attrs", sort_attrs;
}

impl Plugin {
    /// The human readable name, as shown in the GUI.
    pub fn name(self) -> &'static str {
        esvg_rs::PLUGIN_NAMES[self as usize]
    }

    /// Whether this plugin is switched on in `config`.
    pub fn is_enabled(self, config: &PluginConfig) -> bool {
        *self.flag(config)
    }
}

impl FromStr for Plugin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Plugin::ALL
            .into_iter()
            .find(|plugin| plugin.id() == s)
            .ok_or_else(|| {
                let valid: Vec<&str> = Plugin::ALL.iter().map(|plugin| plugin.id()).collect();
                format!("unknown plugin '{s}'. Valid plugins: {}", valid.join(", "))
            })
    }
}

/// The user's plugin choices, as given on the command line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Selection {
    /// When set, only these plugins run. Every other plugin is off.
    pub only: Option<Vec<Plugin>>,
    /// Plugins to switch on, on top of the defaults.
    pub enable: Vec<Plugin>,
    /// Plugins to switch off. Applied last, so it wins over `enable`.
    pub disable: Vec<Plugin>,
}

/// Builds the optimizer configuration for `selection`. Without any choices this
/// is `PluginConfig::default()`, so the CLI matches the GUI's defaults.
pub fn build_config(selection: &Selection) -> PluginConfig {
    let mut config = PluginConfig::default();

    if let Some(only) = &selection.only {
        for plugin in Plugin::ALL {
            *plugin.flag_mut(&mut config) = false;
        }
        for plugin in only {
            *plugin.flag_mut(&mut config) = true;
        }
    }
    for plugin in &selection.enable {
        *plugin.flag_mut(&mut config) = true;
    }
    for plugin in &selection.disable {
        *plugin.flag_mut(&mut config) = false;
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(plugins: &[Plugin]) -> PluginConfig {
        build_config(&Selection {
            only: Some(plugins.to_vec()),
            ..Selection::default()
        })
    }

    fn enabled(config: &PluginConfig) -> Vec<Plugin> {
        Plugin::ALL
            .into_iter()
            .filter(|plugin| plugin.is_enabled(config))
            .collect()
    }

    #[test]
    fn ids_are_unique_and_kebab_case() {
        let ids: Vec<&str> = Plugin::ALL.iter().map(|plugin| plugin.id()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), Plugin::ALL.len());

        for id in ids {
            assert!(!id.is_empty());
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{id} is not kebab-case"
            );
            assert!(!id.starts_with('-') && !id.ends_with('-') && !id.contains("--"));
        }
    }

    #[test]
    fn names_match_the_optimizer_plugin_list_in_order() {
        let names: Vec<&str> = Plugin::ALL.iter().map(|plugin| plugin.name()).collect();
        assert_eq!(names, esvg_rs::PLUGIN_NAMES);
    }

    #[test]
    fn every_id_parses_back_to_its_plugin() {
        for plugin in Plugin::ALL {
            assert_eq!(plugin.id().parse::<Plugin>(), Ok(plugin));
        }
    }

    #[test]
    fn unknown_id_error_lists_valid_ids() {
        let error = "shape-to-paht".parse::<Plugin>().unwrap_err();
        assert!(error.contains("'shape-to-paht'"));
        assert!(error.contains("shape-to-path"));
        assert!(error.contains("sort-attrs"));
    }

    #[test]
    fn display_names_are_not_accepted_as_ids() {
        assert!("Shape to Path".parse::<Plugin>().is_err());
        assert!("ShapeToPath".parse::<Plugin>().is_err());
    }

    #[test]
    fn default_selection_matches_optimizer_defaults() {
        let config = build_config(&Selection::default());
        let default = PluginConfig::default();
        for plugin in Plugin::ALL {
            assert_eq!(
                plugin.is_enabled(&config),
                plugin.is_enabled(&default),
                "{} differs from the default",
                plugin.id()
            );
        }
        assert_eq!(enabled(&config).len(), Plugin::ALL.len());
    }

    #[test]
    fn only_enables_exactly_the_listed_plugins() {
        for plugin in Plugin::ALL {
            assert_eq!(enabled(&only(&[plugin])), vec![plugin]);
        }
        assert!(enabled(&only(&[])).is_empty());
    }

    #[test]
    fn enable_adds_to_the_defaults() {
        let config = build_config(&Selection {
            enable: vec![Plugin::ShapeToPath],
            ..Selection::default()
        });
        assert_eq!(enabled(&config).len(), Plugin::ALL.len());
    }

    #[test]
    fn disable_removes_from_the_defaults() {
        for plugin in Plugin::ALL {
            let config = build_config(&Selection {
                disable: vec![plugin],
                ..Selection::default()
            });
            let expected: Vec<Plugin> = Plugin::ALL
                .into_iter()
                .filter(|other| *other != plugin)
                .collect();
            assert_eq!(enabled(&config), expected);
        }
    }

    #[test]
    fn disable_wins_over_enable_for_the_same_plugin() {
        let config = build_config(&Selection {
            only: Some(vec![]),
            enable: vec![Plugin::SortAttrs],
            disable: vec![Plugin::SortAttrs],
        });
        assert!(!Plugin::SortAttrs.is_enabled(&config));
    }

    #[test]
    fn only_then_enable_adds_plugins_to_the_list() {
        let config = build_config(&Selection {
            only: Some(vec![Plugin::SimplifyPaths]),
            enable: vec![Plugin::SortAttrs],
            disable: vec![],
        });
        assert_eq!(
            enabled(&config),
            vec![Plugin::SimplifyPaths, Plugin::SortAttrs]
        );
    }
}
