//! Runs a command from start to finish and works out the exit code.

use crate::cli::{Command, OptimizeArgs, PluginsArgs};
use crate::inputs::{self, Input};
use crate::output::{self, Destination, Target};
use crate::plugins::{self, Plugin};
use crate::report::{self, FileReport};
use esvg_rs::PluginConfig;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

pub const EXIT_OK: u8 = 0;
pub const EXIT_FAILED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;

/// Runs `command` and returns the process exit code.
pub fn execute(command: Command) -> u8 {
    match command {
        Command::Optimize(args) => optimize(args),
        Command::Plugins(args) => list_plugins(args),
    }
}

fn usage_error(message: &str) -> u8 {
    eprintln!("error: {message}");
    EXIT_USAGE
}

fn optimize(args: OptimizeArgs) -> u8 {
    let config = plugins::build_config(&args.selection());

    let inputs = match inputs::expand(&args.inputs, args.recursive) {
        Ok(inputs) => inputs,
        Err(message) => return usage_error(&message),
    };
    let target = args.target();
    // A dry run writes nothing, so it has no need for an output location.
    let planned = if args.dry_run && target == Target::Stdout {
        Ok(vec![None; inputs.len()])
    } else {
        output::plan(&inputs, &target)
    };
    let destinations = match planned {
        Ok(destinations) => destinations,
        Err(message) => return usage_error(&message),
    };

    let writes_to_stdout = destinations.iter().any(|d| d.is_none()) && !args.dry_run;
    if args.json && writes_to_stdout {
        return usage_error(
            "--json cannot be combined with optimized SVG on stdout. Use -o FILE, -d DIR or -i",
        );
    }

    let jobs = args
        .jobs
        .unwrap_or_else(|| thread::available_parallelism().map_or(1, |n| n.get()));
    let work: Vec<(&Input, &Destination)> = inputs.iter().zip(destinations.iter()).collect();
    let results = map_parallel(&work, jobs, |(input, destination)| {
        process(input, destination.as_deref(), &config, args.dry_run)
    });

    let mut reports = Vec::with_capacity(results.len());
    let mut stdout_payload = None;
    for (report, payload) in results {
        reports.push(report);
        if payload.is_some() {
            stdout_payload = payload;
        }
    }

    let failed = reports.iter().any(|r| r.status == report::Status::Error);

    if let Some(payload) = &stdout_payload
        && io::stdout().write_all(payload.as_bytes()).is_err()
    {
        eprintln!("error: could not write to stdout");
        return EXIT_FAILED;
    }

    if args.json {
        println!("{}", report::json(&reports, args.dry_run));
    } else if !args.quiet || failed {
        let text = report::human(&reports, args.dry_run);
        // Report on stderr when stdout carries the optimized SVG, so pipes stay clean.
        if stdout_payload.is_some() || writes_to_stdout {
            eprint!("{text}");
        } else {
            print!("{text}");
        }
    }

    if failed { EXIT_FAILED } else { EXIT_OK }
}

/// Optimizes one input. Returns the report, and the optimized SVG when it goes to stdout.
fn process(
    input: &Input,
    destination: Option<&Path>,
    config: &PluginConfig,
    dry_run: bool,
) -> (FileReport, Option<String>) {
    let name = input.display();

    let source = match read_input(input) {
        Ok(source) => source,
        Err(message) => return (FileReport::failed(name, None, message), None),
    };
    let input_bytes = source.len() as u64;

    let optimized = match catch_panic(|| esvg_rs::optimize(&source, config)) {
        Ok(optimized) => optimized,
        Err(message) => {
            return (FileReport::failed(name, Some(input_bytes), message), None);
        }
    };
    let output_bytes = optimized.len() as u64;

    if dry_run {
        return (
            FileReport::optimized(name, None, input_bytes, output_bytes),
            None,
        );
    }

    match destination {
        None => {
            let report =
                FileReport::optimized(name, Some("-".to_string()), input_bytes, output_bytes);
            (report, Some(optimized))
        }
        Some(path) => match output::write_atomic(path, optimized.as_bytes()) {
            Ok(()) => {
                let shown = path.display().to_string();
                (
                    FileReport::optimized(name, Some(shown), input_bytes, output_bytes),
                    None,
                )
            }
            Err(e) => (
                FileReport::failed(
                    name,
                    Some(input_bytes),
                    format!("could not write '{}': {e}", path.display()),
                ),
                None,
            ),
        },
    }
}

/// Runs `f`, turning a panic into an error, so one file that crashes the
/// optimizer fails on its own instead of ending the whole run.
fn catch_panic<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown cause".to_string());
        Err(format!("the optimizer crashed on this file: {detail}"))
    })
}

fn read_input(input: &Input) -> Result<String, String> {
    match input {
        Input::Stdin => {
            let mut buffer = Vec::new();
            io::stdin()
                .read_to_end(&mut buffer)
                .map_err(|e| format!("could not read stdin: {e}"))?;
            String::from_utf8(buffer).map_err(|_| "stdin is not valid UTF-8".to_string())
        }
        Input::File(path) => {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("could not read '{}': {e}", path.display()))?;
            String::from_utf8(bytes).map_err(|_| format!("'{}' is not valid UTF-8", path.display()))
        }
    }
}

/// Applies `f` to every item on up to `jobs` threads. Results keep the order of `items`.
pub fn map_parallel<T, R, F>(items: &[T], jobs: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let next = AtomicUsize::new(0);
    let workers = jobs.clamp(1, items.len().max(1));

    let mut indexed: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else { break };
                        done.push((index, f(item)));
                    }
                    done
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("worker thread panicked"))
            .collect()
    });

    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

fn list_plugins(args: PluginsArgs) -> u8 {
    let defaults = PluginConfig::default();

    if args.json {
        let list: Vec<serde_json::Value> = Plugin::ALL
            .iter()
            .map(|plugin| {
                serde_json::json!({
                    "id": plugin.id(),
                    "name": plugin.name(),
                    "default": plugin.is_enabled(&defaults),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).expect("plain data")
        );
        return EXIT_OK;
    }

    let width = Plugin::ALL.iter().map(|p| p.id().len()).max().unwrap_or(0);
    println!("{:<width$}  DEFAULT  NAME", "ID");
    for plugin in Plugin::ALL {
        let default = if plugin.is_enabled(&defaults) {
            "on"
        } else {
            "off"
        };
        println!("{:<width$}  {:<7}  {}", plugin.id(), default, plugin.name());
    }
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn catch_panic_turns_a_panic_into_an_error() {
        let result: Result<(), String> = catch_panic(|| panic!("index out of bounds"));
        let error = result.unwrap_err();
        assert!(error.contains("optimizer crashed"), "{error}");
        assert!(error.contains("index out of bounds"), "{error}");

        let formatted: Result<(), String> = catch_panic(|| panic!("bad value {}", 7));
        assert!(formatted.unwrap_err().contains("bad value 7"));
    }

    #[test]
    fn catch_panic_passes_results_through() {
        assert_eq!(catch_panic(|| Ok::<_, String>(5)), Ok(5));
        assert_eq!(
            catch_panic(|| Err::<u8, _>("no".to_string())),
            Err("no".to_string())
        );
    }

    #[test]
    fn map_parallel_keeps_input_order() {
        let items: Vec<usize> = (0..500).collect();
        let doubled = map_parallel(&items, 8, |n| n * 2);
        assert_eq!(doubled, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn map_parallel_runs_each_item_exactly_once() {
        let items: Vec<usize> = (0..200).collect();
        let calls = AtomicUsize::new(0);
        let results = map_parallel(&items, 4, |_| calls.fetch_add(1, Ordering::SeqCst));
        assert_eq!(calls.load(Ordering::SeqCst), 200);
        let mut seen = results.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 200);
    }

    #[test]
    fn map_parallel_handles_no_items_and_zero_jobs() {
        let empty: Vec<u8> = Vec::new();
        assert!(map_parallel(&empty, 4, |_| 1u8).is_empty());
        assert_eq!(map_parallel(&[1u8, 2], 0, |n| *n), vec![1, 2]);
    }

    #[test]
    fn map_parallel_with_one_job_is_sequential() {
        let flag = AtomicBool::new(false);
        let order = map_parallel(&[1, 2, 3], 1, |n| {
            flag.store(true, Ordering::SeqCst);
            *n
        });
        assert!(flag.load(Ordering::SeqCst));
        assert_eq!(order, vec![1, 2, 3]);
    }
}
