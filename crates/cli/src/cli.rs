//! Command line arguments.

use crate::output::Target;
use crate::plugins::{Plugin, Selection};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "esvg",
    bin_name = "esvg",
    version,
    about = "Optimize SVG files without changing how they look",
    long_about = "Optimize SVG files without changing how they look.\n\n\
        Every plugin runs by default. Use --disable, --enable or --only to choose plugins.\n\n\
        Exit codes: 0 = all files optimized, 1 = at least one file failed, 2 = bad usage.",
    propagate_version = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Optimize one or more SVG files
    #[command(visible_alias = "opt")]
    Optimize(OptimizeArgs),
    /// List the optimization plugins and whether they run by default
    Plugins(PluginsArgs),
}

#[derive(Debug, Args)]
pub struct OptimizeArgs {
    /// Files, directories, glob patterns, or `-` for stdin.
    ///
    /// Quote glob patterns so the shell does not expand them first, e.g. "icons/*.svg".
    /// Directories need --recursive.
    #[arg(required = true, value_name = "INPUT")]
    pub inputs: Vec<String>,

    /// Write the result to FILE, or to stdout with `-`. Only for a single input
    #[arg(short, long, value_name = "FILE", conflicts_with_all = ["out_dir", "in_place"])]
    pub output: Option<String>,

    /// Write each result into DIR, keeping its file name
    #[arg(short = 'd', long, value_name = "DIR", conflicts_with = "in_place")]
    pub out_dir: Option<PathBuf>,

    /// Overwrite the input files with the optimized result
    #[arg(short, long)]
    pub in_place: bool,

    /// Run only these plugins (comma separated ids, see `esvg plugins`)
    #[arg(
        long,
        value_name = "PLUGINS",
        value_delimiter = ',',
        value_parser = parse_plugin,
        conflicts_with_all = ["enable", "disable"]
    )]
    pub only: Option<Vec<Plugin>>,

    /// Turn these plugins on, in addition to the defaults. Repeatable
    #[arg(long, value_name = "PLUGINS", value_delimiter = ',', value_parser = parse_plugin)]
    pub enable: Vec<Plugin>,

    /// Turn these plugins off. Repeatable, and wins over --enable
    #[arg(long, value_name = "PLUGINS", value_delimiter = ',', value_parser = parse_plugin)]
    pub disable: Vec<Plugin>,

    /// Optimize the SVG files inside directories, at any depth
    #[arg(short, long)]
    pub recursive: bool,

    /// Show what would change without writing any file
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Print the report as JSON instead of text
    #[arg(long)]
    pub json: bool,

    /// Hide the text report when every file succeeds. Errors and --json output are still printed
    #[arg(short, long)]
    pub quiet: bool,

    /// Number of files to optimize at once [default: number of CPU cores]
    #[arg(short, long, value_name = "N", value_parser = parse_jobs)]
    pub jobs: Option<usize>,
}

#[derive(Debug, Args)]
pub struct PluginsArgs {
    /// Print the plugin list as JSON
    #[arg(long)]
    pub json: bool,
}

fn parse_plugin(value: &str) -> Result<Plugin, String> {
    value.parse()
}

fn parse_jobs(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(0) | Err(_) => Err(format!(
            "'{value}' is not a valid job count. Use a whole number of 1 or more"
        )),
        Ok(jobs) => Ok(jobs),
    }
}

impl OptimizeArgs {
    pub fn selection(&self) -> Selection {
        Selection {
            only: self.only.clone(),
            enable: self.enable.clone(),
            disable: self.disable.clone(),
        }
    }

    pub fn target(&self) -> Target {
        if self.in_place {
            Target::InPlace
        } else if let Some(dir) = &self.out_dir {
            Target::Dir(dir.clone())
        } else {
            match self.output.as_deref() {
                None | Some("-") => Target::Stdout,
                Some(path) => Target::File(PathBuf::from(path)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("esvg").chain(args.iter().copied()))
    }

    fn optimize(args: &[&str]) -> OptimizeArgs {
        match parse(args).unwrap().command {
            Command::Optimize(args) => args,
            other => panic!("expected optimize, got {other:?}"),
        }
    }

    #[test]
    fn optimize_needs_at_least_one_input() {
        assert!(parse(&["optimize"]).is_err());
    }

    #[test]
    fn plugin_lists_accept_commas_and_repeats() {
        let args = optimize(&[
            "optimize",
            "a.svg",
            "--disable",
            "sort-attrs,shape-to-path",
            "--disable",
            "mangle-ids",
        ]);
        assert_eq!(
            args.disable,
            vec![Plugin::SortAttrs, Plugin::ShapeToPath, Plugin::MangleIds]
        );
    }

    #[test]
    fn unknown_plugin_is_a_usage_error_that_lists_valid_ids() {
        let error = parse(&["optimize", "a.svg", "--disable", "nope"]).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("unknown plugin 'nope'"), "{text}");
        assert!(text.contains("simplify-paths"), "{text}");
    }

    #[test]
    fn only_cannot_be_combined_with_enable_or_disable() {
        assert!(
            parse(&[
                "optimize",
                "a.svg",
                "--only",
                "sort-attrs",
                "--disable",
                "mangle-ids"
            ])
            .is_err()
        );
        assert!(
            parse(&[
                "optimize",
                "a.svg",
                "--only",
                "sort-attrs",
                "--enable",
                "mangle-ids"
            ])
            .is_err()
        );
    }

    #[test]
    fn output_cannot_be_combined_with_out_dir_or_in_place() {
        assert!(parse(&["optimize", "a.svg", "-o", "x.svg", "-d", "out"]).is_err());
        assert!(parse(&["optimize", "a.svg", "-o", "x.svg", "-i"]).is_err());
        assert!(parse(&["optimize", "a.svg", "-d", "out", "-i"]).is_err());
    }

    #[test]
    fn jobs_must_be_at_least_one() {
        assert!(parse(&["optimize", "a.svg", "-j", "0"]).is_err());
        assert_eq!(optimize(&["optimize", "a.svg", "-j", "3"]).jobs, Some(3));
    }

    #[test]
    fn target_defaults_to_stdout() {
        assert_eq!(optimize(&["optimize", "a.svg"]).target(), Target::Stdout);
        assert_eq!(
            optimize(&["optimize", "a.svg", "-o", "-"]).target(),
            Target::Stdout
        );
    }

    #[test]
    fn target_follows_the_output_flags() {
        assert_eq!(
            optimize(&["optimize", "a.svg", "-o", "x.svg"]).target(),
            Target::File(PathBuf::from("x.svg"))
        );
        assert_eq!(
            optimize(&["optimize", "a.svg", "-d", "out"]).target(),
            Target::Dir(PathBuf::from("out"))
        );
        assert_eq!(
            optimize(&["optimize", "a.svg", "-i"]).target(),
            Target::InPlace
        );
    }

    #[test]
    fn selection_carries_the_plugin_choices() {
        let args = optimize(&["optimize", "a.svg", "--enable", "sort-attrs"]);
        let selection = args.selection();
        assert_eq!(selection.only, None);
        assert_eq!(selection.enable, vec![Plugin::SortAttrs]);
        assert!(selection.disable.is_empty());
    }

    #[test]
    fn plugins_subcommand_parses_json_flag() {
        match parse(&["plugins", "--json"]).unwrap().command {
            Command::Plugins(args) => assert!(args.json),
            other => panic!("expected plugins, got {other:?}"),
        }
    }
}
