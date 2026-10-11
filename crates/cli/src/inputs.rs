//! Turns the INPUT arguments into the list of files to optimize.
//!
//! An argument can be a file, a directory (with `--recursive`), a glob pattern
//! such as `icons/*.svg` (quoted, or expanded by the shell), or `-` for stdin.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Stdin,
    File(PathBuf),
}

impl Input {
    /// The name shown in reports and JSON output.
    pub fn display(&self) -> String {
        match self {
            Input::Stdin => "<stdin>".to_string(),
            Input::File(path) => path.display().to_string(),
        }
    }
}

/// Expands every INPUT argument. Files named explicitly are kept whatever their
/// extension. Files found by a glob or a directory walk must end in `.svg`.
/// The same file given twice is returned once, at its first position.
pub fn expand(args: &[String], recursive: bool) -> Result<Vec<Input>, String> {
    let mut inputs = Vec::new();
    let mut seen = HashSet::new();

    for arg in args {
        if arg == "-" {
            if inputs.contains(&Input::Stdin) {
                return Err("standard input (`-`) can only be given once".to_string());
            }
            inputs.push(Input::Stdin);
            continue;
        }

        for path in expand_one(arg, recursive)? {
            let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(key) {
                inputs.push(Input::File(path));
            }
        }
    }

    Ok(inputs)
}

fn expand_one(arg: &str, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let path = Path::new(arg);

    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if path.is_dir() {
        if !recursive {
            return Err(format!(
                "'{arg}' is a directory. Pass --recursive to optimize the SVG files inside it"
            ));
        }
        let mut found = Vec::new();
        walk_svgs(path, &mut found)
            .map_err(|e| format!("could not read directory '{arg}': {e}"))?;
        if found.is_empty() {
            return Err(format!("no SVG files found in '{arg}'"));
        }
        return Ok(found);
    }

    if has_glob_syntax(arg) {
        return expand_glob(arg);
    }

    Err(format!("'{arg}' does not exist"))
}

fn has_glob_syntax(arg: &str) -> bool {
    arg.contains(['*', '?', '['])
}

fn expand_glob(pattern: &str) -> Result<Vec<PathBuf>, String> {
    let entries = glob::glob(pattern).map_err(|e| format!("invalid pattern '{pattern}': {e}"))?;

    let mut found = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("could not read '{}': {e}", e.path().display()))?;
        if path.is_file() && is_svg(&path) {
            found.push(path);
        }
    }

    if found.is_empty() {
        return Err(format!("no SVG files match '{pattern}'"));
    }
    Ok(found)
}

/// Collects SVG files below `dir`, sorted by path so runs are repeatable.
/// Symbolic links are skipped, so a walk never leaves the directory it started in.
fn walk_svgs(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            walk_svgs(&path, found)?;
        } else if file_type.is_file() && is_svg(&path) {
            found.push(path);
        }
    }
    Ok(())
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &Path, relative: &str) -> PathBuf {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        path
    }

    fn arg(path: &Path) -> String {
        path.display().to_string()
    }

    /// Glob patterns use `/` on every platform, so a Windows path works too.
    fn pattern(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    fn files(inputs: &[Input]) -> Vec<PathBuf> {
        inputs
            .iter()
            .filter_map(|input| match input {
                Input::File(path) => Some(path.clone()),
                Input::Stdin => None,
            })
            .collect()
    }

    #[test]
    fn explicit_file_is_kept_whatever_its_extension() {
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "notes.txt");
        let inputs = expand(&[arg(&path)], false).unwrap();
        assert_eq!(inputs, vec![Input::File(path)]);
    }

    #[test]
    fn dash_is_stdin() {
        let inputs = expand(&["-".to_string()], false).unwrap();
        assert_eq!(inputs, vec![Input::Stdin]);
        assert_eq!(Input::Stdin.display(), "<stdin>");
    }

    #[test]
    fn dash_twice_is_an_error() {
        let error = expand(&["-".to_string(), "-".to_string()], false).unwrap_err();
        assert!(error.contains("only be given once"), "{error}");
    }

    #[test]
    fn missing_path_is_an_error() {
        let dir = TempDir::new().unwrap();
        let error = expand(&[arg(&dir.path().join("nope.svg"))], false).unwrap_err();
        assert!(error.contains("does not exist"), "{error}");
    }

    #[test]
    fn directory_needs_recursive_flag() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.svg");
        let error = expand(&[arg(dir.path())], false).unwrap_err();
        assert!(error.contains("--recursive"), "{error}");
    }

    #[test]
    fn recursive_walk_finds_svgs_at_every_depth_in_sorted_order() {
        let dir = TempDir::new().unwrap();
        let b = write(dir.path(), "b.svg");
        let a_nested = write(dir.path(), "sub/a.svg");
        let upper = write(dir.path(), "sub/deeper/C.SVG");
        write(dir.path(), "readme.md");

        let inputs = expand(&[arg(dir.path())], true).unwrap();
        let found = files(&inputs);
        assert_eq!(found.len(), 3);
        assert!(found.contains(&b));
        assert!(found.contains(&a_nested));
        assert!(found.contains(&upper));
        let mut sorted = found.clone();
        sorted.sort();
        assert_eq!(found, sorted);
    }

    #[test]
    fn directory_without_svg_files_is_an_error() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "notes/readme.md");
        let error = expand(&[arg(dir.path())], true).unwrap_err();
        assert!(error.contains("no SVG files found"), "{error}");
    }

    #[test]
    fn glob_matches_only_svg_files() {
        let dir = TempDir::new().unwrap();
        let one = write(dir.path(), "one.svg");
        let two = write(dir.path(), "two.svg");
        write(dir.path(), "three.png");

        let pattern = pattern(&dir.path().join("*"));
        let inputs = expand(&[pattern], false).unwrap();
        let mut found = files(&inputs);
        found.sort();
        assert_eq!(found, vec![one, two]);
    }

    #[test]
    fn recursive_glob_reaches_nested_folders() {
        let dir = TempDir::new().unwrap();
        let nested = write(dir.path(), "icons/ui/close.svg");
        let pattern = pattern(&dir.path().join("**").join("*.svg"));
        let inputs = expand(&[pattern], false).unwrap();
        assert_eq!(files(&inputs), vec![nested]);
    }

    #[test]
    fn glob_with_no_svg_matches_is_an_error() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "image.png");
        let pattern = pattern(&dir.path().join("*.svg"));
        let error = expand(std::slice::from_ref(&pattern), false).unwrap_err();
        assert!(error.contains("no SVG files match"), "{error}");
        assert!(error.contains(&pattern));
    }

    #[test]
    fn invalid_glob_is_an_error() {
        let error = expand(&["[unclosed*.svg".to_string()], false).unwrap_err();
        assert!(
            error.contains("invalid pattern") || error.contains("does not exist"),
            "{error}"
        );
    }

    #[test]
    fn same_file_given_twice_is_optimized_once() {
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "a.svg");
        let direct = arg(&path);
        let glob = pattern(&dir.path().join("*.svg"));
        let inputs = expand(&[direct, glob], false).unwrap();
        assert_eq!(inputs.len(), 1);
    }

    #[test]
    fn order_of_explicit_arguments_is_kept() {
        let dir = TempDir::new().unwrap();
        let z = write(dir.path(), "z.svg");
        let a = write(dir.path(), "a.svg");
        let inputs = expand(&[arg(&z), arg(&a)], false).unwrap();
        assert_eq!(files(&inputs), vec![z, a]);
    }

    #[cfg(unix)]
    #[test]
    fn walk_skips_symbolic_links() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        write(outside.path(), "far.svg");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let real = write(dir.path(), "real.svg");

        let inputs = expand(&[arg(dir.path())], true).unwrap();
        assert_eq!(files(&inputs), vec![real]);
    }
}
