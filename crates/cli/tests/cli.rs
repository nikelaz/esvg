//! End-to-end tests that run the real `esvg-cli` binary.
//!
//! Each test works in its own temporary folder. Output is compared with the
//! library's `optimize` so the CLI cannot drift from what the GUI produces.

use esvg_cli::plugins::Plugin;
use esvg_rs::{PluginConfig, optimize};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;
use xmltree::{Element, XMLNode};

const RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" fill="#ff0000"/></svg>"##;

const MESSY: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- a comment -->
<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48">
  <g id="layer1" transform="translate(0 0)">
    <path d="M 10.000000 10.000000 L 20.000000 10.000000 L 30.000000 10.000000 L 30.000000 30.000000 L 10.000000 30.000000 Z" fill="#000000" style="stroke:none"/>
    <circle cx="24.5" cy="24.5" r="5" fill="#ffffff"/>
  </g>
</svg>
"##;

const BROKEN: &str = "<svg";

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_in(dir: &Path, args: &[&str]) -> Run {
    collect(
        Command::new(env!("CARGO_BIN_EXE_esvg-cli"))
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to start esvg-cli"),
    )
}

fn run_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_esvg-cli"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start esvg-cli");
    // A usage error makes the CLI exit before it reads stdin, which closes the pipe.
    match child.stdin.take().unwrap().write_all(stdin.as_bytes()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            panic!("could not write stdin: {e}")
        }
        _ => {}
    }
    collect(child.wait_with_output().unwrap())
}

fn collect(output: std::process::Output) -> Run {
    Run {
        code: output.status.code().expect("terminated by signal"),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn write(dir: &Path, relative: &str, contents: &str) -> PathBuf {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

fn library_default(source: &str) -> String {
    optimize(source, &PluginConfig::default()).unwrap()
}

/// The markup with attributes in name order. The optimizer writes attributes in
/// an arbitrary order that changes between runs, so tests compare this form.
fn canonical(svg: &str) -> String {
    fn walk(element: &Element, out: &mut String) {
        out.push('<');
        out.push_str(&element.name);
        let mut attributes: Vec<_> = element.attributes.iter().collect();
        attributes.sort();
        for (name, value) in attributes {
            out.push_str(&format!(" {name}=\"{value}\""));
        }
        out.push('>');
        for child in &element.children {
            match child {
                XMLNode::Element(child) => walk(child, out),
                XMLNode::Text(text) => out.push_str(text),
                _ => {}
            }
        }
        out.push_str(&format!("</{}>", element.name));
    }

    let root = Element::parse(svg.as_bytes()).expect("output is well-formed XML");
    let mut out = String::new();
    walk(&root, &mut out);
    out
}

fn assert_same_svg(actual: &str, expected: &str) {
    assert_eq!(canonical(actual), canonical(expected));
}

/// A config with every plugin off. Tests turn on the plugins they need.
fn all_off() -> PluginConfig {
    let mut config = PluginConfig::default();
    for plugin in Plugin::ALL {
        set(&mut config, plugin, false);
    }
    config
}

fn set(config: &mut PluginConfig, plugin: Plugin, on: bool) {
    match plugin {
        Plugin::ApplyTransforms => config.apply_transforms = on,
        Plugin::CollapseGroups => config.collapse_groups = on,
        Plugin::CombinePaths => config.combine_paths = on,
        Plugin::CssToAttributes => config.css_to_attributes = on,
        Plugin::MangleIds => config.mangle_ids = on,
        Plugin::NumberPrecision => config.number_precision = on,
        Plugin::OptimizeColors => config.optimize_colors = on,
        Plugin::RemoveEmptyText => config.remove_empty_text = on,
        Plugin::RemoveUnnecessaryAttrs => config.remove_unnecessary_attrs = on,
        Plugin::RemoveUnnecessaryClipPaths => config.remove_unnecessary_clip_paths = on,
        Plugin::ShapeToPath => config.shape_to_path = on,
        Plugin::SimplifyPaths => config.simplify_paths = on,
        Plugin::SortAttrs => config.sort_attrs = on,
    }
}

fn svg_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------- stdout and files

#[test]
fn single_file_goes_to_stdout_and_report_goes_to_stderr() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(dir.path(), &["optimize", "rect.svg"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&run.stdout, &library_default(RECT));
    assert!(run.stderr.contains("rect.svg: "), "{}", run.stderr);
    assert!(
        run.stderr.contains("1 optimized, 0 failed"),
        "{}",
        run.stderr
    );
}

#[test]
fn stdout_output_is_exactly_the_optimized_svg() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(dir.path(), &["optimize", "messy.svg", "-q"]);

    assert_eq!(run.code, 0);
    assert_same_svg(&run.stdout, &library_default(MESSY));
    assert!(run.stderr.is_empty());
}

#[test]
fn stdin_to_stdout_optimizes_the_piped_svg() {
    let dir = TempDir::new().unwrap();
    let run = run_with_stdin(dir.path(), &["optimize", "-"], MESSY);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&run.stdout, &library_default(MESSY));
    assert!(run.stderr.contains("<stdin>: "), "{}", run.stderr);
}

#[test]
fn output_flag_writes_the_file_and_reports_on_stdout() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(dir.path(), &["optimize", "rect.svg", "-o", "small.svg"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&read(&dir.path().join("small.svg")), &library_default(RECT));
    assert!(run.stdout.contains("-> small.svg"), "{}", run.stdout);
    assert!(
        run.stdout.contains("1 optimized, 0 failed"),
        "{}",
        run.stdout
    );
}

#[test]
fn output_dash_means_stdout() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(dir.path(), &["optimize", "rect.svg", "-o", "-", "-q"]);

    assert_eq!(run.code, 0);
    assert_same_svg(&run.stdout, &library_default(RECT));
}

#[test]
fn optimized_output_is_smaller_than_the_input_for_messy_files() {
    let dir = TempDir::new().unwrap();
    let input = write(dir.path(), "messy.svg", MESSY);
    run_in(
        dir.path(),
        &["optimize", "messy.svg", "-o", "out.svg", "-q"],
    );

    let before = fs::metadata(&input).unwrap().len();
    let after = fs::metadata(dir.path().join("out.svg")).unwrap().len();
    assert!(after < before, "{after} is not smaller than {before}");
}

#[test]
fn out_dir_writes_every_input_under_its_own_name_and_creates_the_folder() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "messy.svg", "-d", "build/icons"],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(
        &read(&dir.path().join("build/icons/rect.svg")),
        &library_default(RECT),
    );
    assert_same_svg(
        &read(&dir.path().join("build/icons/messy.svg")),
        &library_default(MESSY),
    );
    assert!(
        run.stdout.contains("2 optimized, 0 failed"),
        "{}",
        run.stdout
    );
}

#[test]
fn out_dir_refuses_two_inputs_with_the_same_name_before_writing_anything() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "a/icon.svg", RECT);
    write(dir.path(), "b/icon.svg", RECT);

    let run = run_in(
        dir.path(),
        &["optimize", "a/icon.svg", "b/icon.svg", "-d", "out"],
    );

    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("would both be written"),
        "{}",
        run.stderr
    );
    assert!(!dir.path().join("out").exists());
}

#[test]
fn in_place_overwrites_inputs_and_leaves_no_temporary_files() {
    let dir = TempDir::new().unwrap();
    let rect = write(dir.path(), "rect.svg", RECT);
    let messy = write(dir.path(), "messy.svg", MESSY);

    let run = run_in(dir.path(), &["optimize", "rect.svg", "messy.svg", "-i"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&read(&rect), &library_default(RECT));
    assert_same_svg(&read(&messy), &library_default(MESSY));
    assert_eq!(svg_names(dir.path()), vec!["messy.svg", "rect.svg"]);
}

// ---------------------------------------------------------------- inputs

#[test]
fn quoted_glob_optimizes_only_svg_files() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "icons/one.svg", RECT);
    write(dir.path(), "icons/two.svg", MESSY);
    write(dir.path(), "icons/notes.txt", "not an image");

    let run = run_in(dir.path(), &["optimize", "icons/*.svg", "-d", "out"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        svg_names(&dir.path().join("out")),
        vec!["one.svg", "two.svg"]
    );
}

#[test]
fn recursive_flag_optimizes_svg_files_in_nested_folders() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "src/a.svg", RECT);
    write(dir.path(), "src/deep/b.svg", MESSY);
    write(dir.path(), "src/deep/readme.md", "# docs");

    let run = run_in(dir.path(), &["optimize", "src", "-r", "-d", "out"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(svg_names(&dir.path().join("out")), vec!["a.svg", "b.svg"]);
}

#[test]
fn directory_without_recursive_is_a_usage_error_and_writes_nothing() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "src/a.svg", RECT);

    let run = run_in(dir.path(), &["optimize", "src", "-d", "out"]);

    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("--recursive"), "{}", run.stderr);
    assert!(!dir.path().join("out").exists());
}

#[test]
fn missing_input_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    let run = run_in(dir.path(), &["optimize", "nope.svg"]);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("does not exist"), "{}", run.stderr);
}

#[test]
fn empty_directory_is_a_usage_error_not_a_silent_success() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let run = run_in(dir.path(), &["optimize", "empty", "-r", "-d", "out"]);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("no SVG files found"), "{}", run.stderr);
}

#[test]
fn glob_matching_nothing_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "image.png", "binary");
    let run = run_in(dir.path(), &["optimize", "*.svg"]);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("no SVG files match"), "{}", run.stderr);
}

// ---------------------------------------------------------------- usage rules

#[test]
fn several_inputs_without_a_target_are_refused_and_nothing_changes() {
    let dir = TempDir::new().unwrap();
    let rect = write(dir.path(), "rect.svg", RECT);
    let messy = write(dir.path(), "messy.svg", MESSY);

    let run = run_in(dir.path(), &["optimize", "rect.svg", "messy.svg"]);

    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("-d DIR"), "{}", run.stderr);
    assert_eq!(read(&rect), RECT);
    assert_eq!(read(&messy), MESSY);
}

#[test]
fn output_flag_with_several_inputs_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    write(dir.path(), "messy.svg", MESSY);
    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "messy.svg", "-o", "x.svg"],
    );
    assert_eq!(run.code, 2);
    assert!(!dir.path().join("x.svg").exists());
}

#[test]
fn stdin_cannot_be_optimized_in_place() {
    let dir = TempDir::new().unwrap();
    let run = run_with_stdin(dir.path(), &["optimize", "-", "-i"], RECT);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("in place"), "{}", run.stderr);
}

#[test]
fn stdin_given_twice_is_refused_even_in_a_dry_run() {
    let dir = TempDir::new().unwrap();
    let run = run_with_stdin(dir.path(), &["optimize", "-", "-", "--dry-run"], RECT);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("only be given once"), "{}", run.stderr);
}

#[cfg(unix)]
#[test]
fn in_place_on_a_symbolic_link_optimizes_the_target_and_keeps_the_link() {
    let dir = TempDir::new().unwrap();
    let real = write(dir.path(), "shared/logo.svg", MESSY);
    std::os::unix::fs::symlink("shared/logo.svg", dir.path().join("logo.svg")).unwrap();

    let run = run_in(dir.path(), &["optimize", "logo.svg", "-i"]);

    assert_eq!(run.code, 0, "{}", run.stderr);
    let link = fs::symlink_metadata(dir.path().join("logo.svg")).unwrap();
    assert!(link.file_type().is_symlink());
    assert_same_svg(&read(&real), &library_default(MESSY));
}

#[test]
fn json_with_optimized_svg_on_stdout_is_refused() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    let run = run_in(dir.path(), &["optimize", "rect.svg", "--json"]);
    assert_eq!(run.code, 2);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
}

#[test]
fn zero_jobs_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    let run = run_in(dir.path(), &["optimize", "rect.svg", "-j", "0"]);
    assert_eq!(run.code, 2);
}

// ---------------------------------------------------------------- plugins

#[test]
fn disabling_shape_to_path_keeps_the_rect() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let default = run_in(dir.path(), &["optimize", "rect.svg", "-q"]);
    let disabled = run_in(
        dir.path(),
        &["optimize", "rect.svg", "-q", "--disable", "shape-to-path"],
    );

    assert!(default.stdout.contains("<path"), "{}", default.stdout);
    assert!(!default.stdout.contains("<rect"), "{}", default.stdout);
    assert!(disabled.stdout.contains("<rect"), "{}", disabled.stdout);

    let config = PluginConfig {
        shape_to_path: false,
        ..PluginConfig::default()
    };
    assert_same_svg(&disabled.stdout, &optimize(RECT, &config).unwrap());
}

#[test]
fn disable_takes_a_comma_separated_list() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &[
            "optimize",
            "messy.svg",
            "-q",
            "--disable",
            "sort-attrs,mangle-ids",
        ],
    );

    let config = PluginConfig {
        sort_attrs: false,
        mangle_ids: false,
        ..PluginConfig::default()
    };
    assert_same_svg(&run.stdout, &optimize(MESSY, &config).unwrap());
}

#[test]
fn only_runs_exactly_the_listed_plugins() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &["optimize", "messy.svg", "-q", "--only", "sort-attrs"],
    );

    let mut config = all_off();
    config.sort_attrs = true;
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&run.stdout, &optimize(MESSY, &config).unwrap());
}

#[test]
fn enabling_a_plugin_that_is_already_on_changes_nothing() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "-q", "--enable", "shape-to-path"],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_same_svg(&run.stdout, &library_default(RECT));
}

#[test]
fn enable_adds_a_plugin_to_an_only_list() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &["optimize", "messy.svg", "-q", "--only", "sort-attrs"],
    );
    let with_enable = run_in(
        dir.path(),
        &[
            "optimize",
            "messy.svg",
            "-q",
            "--only",
            "sort-attrs",
            "--enable",
            "shape-to-path",
        ],
    );
    assert_eq!(run.code, 0);
    assert_eq!(
        with_enable.code, 2,
        "--only and --enable cannot be combined"
    );
}

#[test]
fn disable_wins_over_enable_for_the_same_plugin() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(
        dir.path(),
        &[
            "optimize",
            "rect.svg",
            "-q",
            "--enable",
            "shape-to-path",
            "--disable",
            "shape-to-path",
        ],
    );

    let config = PluginConfig {
        shape_to_path: false,
        ..PluginConfig::default()
    };
    assert_eq!(run.code, 0);
    assert_same_svg(&run.stdout, &optimize(RECT, &config).unwrap());
}

#[test]
fn unknown_plugin_is_a_usage_error_that_lists_the_valid_ids() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "--disable", "shape-to-pathh"],
    );

    assert_eq!(run.code, 2);
    assert!(
        run.stderr.contains("unknown plugin 'shape-to-pathh'"),
        "{}",
        run.stderr
    );
    assert!(run.stderr.contains("shape-to-path"), "{}", run.stderr);
}

#[test]
fn plugins_command_lists_every_plugin_with_its_default() {
    let dir = TempDir::new().unwrap();
    let run = run_in(dir.path(), &["plugins", "--json"]);

    assert_eq!(run.code, 0);
    let list: Value = serde_json::from_str(&run.stdout).unwrap();
    let entries = list.as_array().unwrap();
    assert_eq!(entries.len(), Plugin::ALL.len());
    for (entry, plugin) in entries.iter().zip(Plugin::ALL) {
        assert_eq!(entry["id"], plugin.id());
        assert_eq!(entry["name"], plugin.name());
        assert_eq!(entry["default"], true);
    }
}

#[test]
fn plugins_command_prints_a_table_for_humans() {
    let dir = TempDir::new().unwrap();
    let run = run_in(dir.path(), &["plugins"]);

    assert_eq!(run.code, 0);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), Plugin::ALL.len() + 1);
    assert!(lines[0].starts_with("ID"), "{}", lines[0]);
    assert!(lines.iter().skip(1).all(|line| line.contains(" on ")));
}

// ---------------------------------------------------------------- failures and reports

#[test]
fn a_broken_file_fails_the_run_but_the_others_are_still_written() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    write(dir.path(), "broken.svg", BROKEN);
    write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &[
            "optimize",
            "rect.svg",
            "broken.svg",
            "messy.svg",
            "-d",
            "out",
        ],
    );

    assert_eq!(run.code, 1);
    assert!(run.stdout.contains("broken.svg: error:"), "{}", run.stdout);
    assert!(
        run.stdout.contains("2 optimized, 1 failed"),
        "{}",
        run.stdout
    );
    assert_eq!(
        svg_names(&dir.path().join("out")),
        vec!["messy.svg", "rect.svg"]
    );
}

#[test]
fn quiet_hides_the_report_on_success_but_not_errors() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    write(dir.path(), "broken.svg", BROKEN);

    let success = run_in(dir.path(), &["optimize", "rect.svg", "-q", "-d", "out"]);
    assert_eq!(success.code, 0);
    assert!(success.stdout.is_empty());
    assert!(success.stderr.is_empty());

    let failure = run_in(dir.path(), &["optimize", "broken.svg", "-q", "-d", "out"]);
    assert_eq!(failure.code, 1);
    assert!(failure.stdout.contains("error:"), "{}", failure.stdout);
}

#[test]
fn json_report_describes_every_file_and_the_summary() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);
    write(dir.path(), "broken.svg", BROKEN);

    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "broken.svg", "-d", "out", "--json"],
    );

    assert_eq!(run.code, 1);
    let report: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["files"][0]["status"], "ok");
    assert_eq!(
        report["files"][0]["output"],
        Path::new("out").join("rect.svg").display().to_string()
    );
    assert_eq!(report["files"][1]["status"], "error");
    assert_eq!(report["files"][1]["output"], Value::Null);
    assert!(!report["files"][1]["error"].as_str().unwrap().is_empty());
    assert_eq!(report["summary"]["files"], 2);
    assert_eq!(report["summary"]["optimized"], 1);
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["summary"]["input_bytes"], RECT.len());
    assert_eq!(
        report["summary"]["output_bytes"],
        library_default(RECT).len()
    );
}

#[test]
fn dry_run_reports_sizes_without_writing_and_accepts_several_inputs() {
    let dir = TempDir::new().unwrap();
    let rect = write(dir.path(), "rect.svg", RECT);
    let messy = write(dir.path(), "messy.svg", MESSY);

    let run = run_in(
        dir.path(),
        &["optimize", "rect.svg", "messy.svg", "-n", "--json"],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    let report: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["files"][0]["output"], Value::Null);
    assert_eq!(
        report["files"][0]["output_bytes"],
        library_default(RECT).len()
    );
    assert_eq!(
        report["files"][1]["output_bytes"],
        library_default(MESSY).len()
    );
    assert_eq!(read(&rect), RECT);
    assert_eq!(read(&messy), MESSY);
    assert_eq!(svg_names(dir.path()), vec!["messy.svg", "rect.svg"]);
}

#[test]
fn dry_run_with_out_dir_writes_nothing() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "rect.svg", RECT);

    let run = run_in(dir.path(), &["optimize", "rect.svg", "-n", "-d", "out"]);

    assert_eq!(run.code, 0);
    assert!(
        run.stdout.contains("[dry run, nothing written]"),
        "{}",
        run.stdout
    );
    assert!(!dir.path().join("out").exists());
}

// ---------------------------------------------------------------- bulk and parallelism

/// Every SVG in the optimizer's own test cases, as absolute paths.
fn corpus() -> (PathBuf, Vec<PathBuf>) {
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/cases");
    let mut files: Vec<PathBuf> = fs::read_dir(&cases)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
        .collect();
    files.sort();
    assert!(
        files.len() > 20,
        "expected the optimizer's test cases, found {}",
        files.len()
    );
    (cases, files)
}

#[test]
fn bulk_run_over_the_corpus_matches_the_library_for_every_file() {
    let (cases, files) = corpus();
    let dir = TempDir::new().unwrap();
    let case_dir = cases.display().to_string();

    let run = run_in(
        dir.path(),
        &["optimize", &case_dir, "-r", "-d", "out", "-q"],
    );

    assert_eq!(run.code, 0, "{}", run.stderr);
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let written = read(&dir.path().join("out").join(&name));
        assert_same_svg(&written, &library_default(&read(file)));
    }
}

#[test]
fn job_count_does_not_change_the_output_or_the_report_order() {
    let (cases, _) = corpus();
    let case_dir = cases.display().to_string();

    let mut results = Vec::new();
    for jobs in ["1", "8"] {
        let dir = TempDir::new().unwrap();
        let run = run_in(
            dir.path(),
            &[
                "optimize", &case_dir, "-r", "-d", "out", "-j", jobs, "--json",
            ],
        );
        assert_eq!(run.code, 0, "{}", run.stderr);
        let report: Value = serde_json::from_str(&run.stdout).unwrap();
        let names: Vec<String> = report["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["input"].as_str().unwrap().to_string())
            .collect();
        let contents = svg_names(&dir.path().join("out"))
            .into_iter()
            .map(|name| canonical(&read(&dir.path().join("out").join(name))))
            .collect::<Vec<_>>();
        results.push((names, contents));
    }

    assert_eq!(results[0], results[1]);
}

#[test]
fn version_flag_prints_the_crate_version() {
    let dir = TempDir::new().unwrap();
    let run = run_in(dir.path(), &["--version"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.starts_with("esvg "), "{}", run.stdout);
}
