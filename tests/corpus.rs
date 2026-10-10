//! Runs the default optimization pipeline over every SVG in `tests/cases` and
//! `tests/corpus`. Each file must optimize without panicking, the output must
//! parse again, and the output must not be larger than the input.
//!
//! Whether the output still *looks* the same is checked by the renderer-based
//! harness in `tests/visual/run.py`, which needs external tools.

use esvg_rs::{optimize, PluginConfig, Svg};
use std::fs;
use std::path::{Path, PathBuf};

fn svg_files(dir: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut files: Vec<PathBuf> = match fs::read_dir(&root) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |e| e == "svg"))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

#[test]
fn every_corpus_file_optimizes_safely() {
    let mut files = svg_files("tests/cases");
    files.extend(svg_files("tests/corpus"));
    assert!(!files.is_empty(), "no test SVGs found");

    let mut failures = Vec::new();
    for path in &files {
        let input = fs::read_to_string(path).expect("readable SVG");

        let output = match optimize(&input, &PluginConfig::default()) {
            Ok(output) => output,
            Err(e) => {
                failures.push(format!("{}: optimize failed: {}", path.display(), e));
                continue;
            }
        };

        if let Err(e) = Svg::from_string(&output) {
            failures.push(format!("{}: output does not parse: {}", path.display(), e));
            continue;
        }

        if output.len() > input.len() {
            failures.push(format!(
                "{}: output grew from {} to {} bytes",
                path.display(),
                input.len(),
                output.len()
            ));
        }
    }

    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
}
