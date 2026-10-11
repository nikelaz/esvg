//! Decides where each optimized file goes, and writes it there.
//!
//! Planning runs before any file is touched, so a bad combination of options
//! fails without writing anything.

use crate::inputs::Input;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Where the optimized output goes, as chosen on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Standard output. Used when no target is given, or with `-o -`.
    Stdout,
    /// A single output file, from `-o FILE`.
    File(PathBuf),
    /// A directory, from `-d DIR`. Each output keeps its file name.
    Dir(PathBuf),
    /// Overwrite each input file, from `-i`.
    InPlace,
}

/// Where one input's output goes. `None` means standard output.
pub type Destination = Option<PathBuf>;

/// Works out the destination of every input, or explains why the combination
/// of inputs and target cannot work.
pub fn plan(inputs: &[Input], target: &Target) -> Result<Vec<Destination>, String> {
    match target {
        Target::Stdout => {
            if inputs.len() > 1 {
                return Err(
                    "several inputs need an output location: pass -d DIR to write each file \
                     into a folder, or -i to overwrite them"
                        .to_string(),
                );
            }
            Ok(vec![None; inputs.len()])
        }

        Target::File(path) => {
            if inputs.len() > 1 {
                return Err(
                    "-o writes one file, so it takes exactly one input. Use -d DIR for several"
                        .to_string(),
                );
            }
            if path.is_dir() {
                return Err(format!(
                    "'{}' is a directory. Use -d to write into it",
                    path.display()
                ));
            }
            Ok(vec![Some(path.clone()); inputs.len()])
        }

        Target::Dir(dir) => {
            let mut destinations = Vec::with_capacity(inputs.len());
            // Keyed without letter case: on macOS and Windows `Icon.svg` and
            // `icon.svg` are the same file, and an output folder may be copied there.
            let mut taken: HashMap<String, (String, PathBuf)> = HashMap::new();

            for input in inputs {
                let Input::File(path) = input else {
                    return Err("standard input has no file name, so it cannot go into -d DIR. Use -o FILE or stdout".to_string());
                };
                let Some(name) = path.file_name() else {
                    return Err(format!("'{}' has no file name", path.display()));
                };
                let destination = dir.join(name);
                let key = destination.to_string_lossy().to_lowercase();
                let entry = (path.display().to_string(), destination.clone());
                if let Some((previous, previous_destination)) = taken.insert(key, entry) {
                    let case_note = if previous_destination == destination {
                        ""
                    } else {
                        " (names that differ only in letter case are the same file on macOS and Windows)"
                    };
                    return Err(format!(
                        "'{previous}' and '{}' would both be written to '{}'{case_note}",
                        path.display(),
                        destination.display()
                    ));
                }
                destinations.push(Some(destination));
            }
            Ok(destinations)
        }

        Target::InPlace => inputs
            .iter()
            .map(|input| match input {
                Input::File(path) => Ok(Some(path.clone())),
                Input::Stdin => Err(
                    "standard input cannot be optimized in place. Use -o FILE or stdout"
                        .to_string(),
                ),
            })
            .collect(),
    }
}

/// Writes `data` to `path` through a temporary file in the same folder, so an
/// interrupted run never leaves a half-written file behind. The new file keeps
/// the permissions of the file it replaces.
///
/// When `path` is a symbolic link, the file it points at is written and the
/// link is left in place.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let path = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let (temp, file) = create_temp_beside(&path)?;
    let result = finish_write(file, &temp, &path, data);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Creates a new, uniquely named file next to `path`. The file must not exist
/// yet, so an existing file or link with that name is never written through.
fn create_temp_beside(path: &Path) -> io::Result<(PathBuf, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy()
        .into_owned();

    for _ in 0..100 {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(".{file_name}.{}-{n}.esvg-tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not find a free name for a temporary file",
    ))
}

fn finish_write(mut file: File, temp: &Path, path: &Path, data: &[u8]) -> io::Result<()> {
    file.write_all(data)?;
    drop(file);
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(temp, metadata.permissions())?;
    }
    fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn files(paths: &[&str]) -> Vec<Input> {
        paths
            .iter()
            .map(|p| Input::File(PathBuf::from(p)))
            .collect()
    }

    #[test]
    fn stdout_target_allows_one_input() {
        let destinations = plan(&files(&["a.svg"]), &Target::Stdout).unwrap();
        assert_eq!(destinations, vec![None]);
    }

    #[test]
    fn stdout_target_rejects_several_inputs_without_writing() {
        let error = plan(&files(&["a.svg", "b.svg"]), &Target::Stdout).unwrap_err();
        assert!(error.contains("-d DIR"), "{error}");
        assert!(error.contains("-i"), "{error}");
    }

    #[test]
    fn output_file_rejects_several_inputs() {
        let target = Target::File(PathBuf::from("out.svg"));
        let error = plan(&files(&["a.svg", "b.svg"]), &target).unwrap_err();
        assert!(error.contains("exactly one input"), "{error}");
    }

    #[test]
    fn output_file_is_used_for_single_input() {
        let target = Target::File(PathBuf::from("out.svg"));
        let destinations = plan(&files(&["a.svg"]), &target).unwrap();
        assert_eq!(destinations, vec![Some(PathBuf::from("out.svg"))]);
    }

    #[test]
    fn output_file_cannot_be_an_existing_directory() {
        let dir = TempDir::new().unwrap();
        let target = Target::File(dir.path().to_path_buf());
        let error = plan(&files(&["a.svg"]), &target).unwrap_err();
        assert!(error.contains("is a directory"), "{error}");
    }

    #[test]
    fn out_dir_keeps_file_names() {
        let target = Target::Dir(PathBuf::from("build"));
        let destinations = plan(&files(&["icons/a.svg", "other/b.svg"]), &target).unwrap();
        assert_eq!(
            destinations,
            vec![
                Some(PathBuf::from("build/a.svg")),
                Some(PathBuf::from("build/b.svg"))
            ]
        );
    }

    #[test]
    fn out_dir_rejects_two_inputs_with_the_same_name() {
        let target = Target::Dir(PathBuf::from("build"));
        let error = plan(&files(&["icons/a.svg", "other/a.svg"]), &target).unwrap_err();
        assert!(error.contains("icons/a.svg"), "{error}");
        assert!(error.contains("other/a.svg"), "{error}");
        let destination = PathBuf::from("build").join("a.svg");
        assert!(error.contains(&destination.display().to_string()), "{error}");
    }

    #[test]
    fn out_dir_rejects_names_that_differ_only_in_case() {
        let target = Target::Dir(PathBuf::from("build"));
        let error = plan(&files(&["a/Icon.svg", "b/icon.svg"]), &target).unwrap_err();
        assert!(error.contains("letter case"), "{error}");
    }

    #[test]
    fn out_dir_rejects_stdin() {
        let target = Target::Dir(PathBuf::from("build"));
        let error = plan(&[Input::Stdin], &target).unwrap_err();
        assert!(error.contains("standard input"), "{error}");
    }

    #[test]
    fn in_place_writes_back_to_each_input() {
        let destinations = plan(&files(&["a.svg", "b.svg"]), &Target::InPlace).unwrap();
        assert_eq!(
            destinations,
            vec![Some(PathBuf::from("a.svg")), Some(PathBuf::from("b.svg"))]
        );
    }

    #[test]
    fn in_place_rejects_stdin() {
        let error = plan(&[Input::Stdin], &Target::InPlace).unwrap_err();
        assert!(error.contains("in place"), "{error}");
    }

    #[test]
    fn stdin_can_go_to_a_named_file() {
        let target = Target::File(PathBuf::from("out.svg"));
        let destinations = plan(&[Input::Stdin], &target).unwrap();
        assert_eq!(destinations, vec![Some(PathBuf::from("out.svg"))]);
    }

    #[test]
    fn write_atomic_creates_missing_folders() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a/b/c.svg");
        write_atomic(&path, b"<svg/>").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"<svg/>");
    }

    #[test]
    fn write_atomic_replaces_existing_file_and_leaves_no_temp_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.svg");
        fs::write(&path, "old").unwrap();
        write_atomic(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.svg".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_keeps_permissions_of_the_replaced_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("script.svg");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        write_atomic(&path, b"new").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn write_atomic_failure_leaves_no_temp_file() {
        let dir = TempDir::new().unwrap();
        // A directory where the file should go makes the final rename fail.
        let path = dir.path().join("taken");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "x").unwrap();

        assert!(write_atomic(&path, b"data").is_err());
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["taken".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_writes_through_a_symbolic_link_and_keeps_it() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("shared/logo.svg");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "old").unwrap();
        let link = dir.path().join("logo.svg");
        std::os::unix::fs::symlink("shared/logo.svg", &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&real).unwrap(), b"new");
    }

    #[test]
    fn temporary_files_get_fresh_names_and_never_reuse_an_existing_one() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.svg");
        let (first, first_file) = create_temp_beside(&path).unwrap();
        let (second, _second_file) = create_temp_beside(&path).unwrap();
        assert_ne!(first, second);

        drop(first_file);
        fs::write(&first, "planted").unwrap();
        let (third, _third_file) = create_temp_beside(&path).unwrap();
        assert_ne!(third, first);
        assert_eq!(fs::read(&first).unwrap(), b"planted");
    }
}
