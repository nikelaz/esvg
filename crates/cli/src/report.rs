//! What the user sees after a run: one line per file, a summary, and JSON for
//! scripts and agents.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Error,
}

/// The outcome for one input file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileReport {
    /// The input as the user wrote it, or `<stdin>`.
    pub input: String,
    /// Where the output went: a path, `-` for stdout, or null when nothing was written.
    pub output: Option<String>,
    pub status: Status,
    pub input_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    /// Bytes saved. Negative when the output is larger.
    pub saved_bytes: Option<i64>,
    /// Percentage of the input saved. Negative when the output is larger.
    pub saved_percent: Option<f64>,
    pub error: Option<String>,
}

impl FileReport {
    pub fn optimized(
        input: String,
        output: Option<String>,
        input_bytes: u64,
        output_bytes: u64,
    ) -> Self {
        FileReport {
            input,
            output,
            status: Status::Ok,
            input_bytes: Some(input_bytes),
            output_bytes: Some(output_bytes),
            saved_bytes: Some(input_bytes as i64 - output_bytes as i64),
            saved_percent: Some(saved_percent(input_bytes, output_bytes)),
            error: None,
        }
    }

    pub fn failed(input: String, input_bytes: Option<u64>, error: String) -> Self {
        FileReport {
            input,
            output: None,
            status: Status::Error,
            input_bytes,
            output_bytes: None,
            saved_bytes: None,
            saved_percent: None,
            error: Some(error),
        }
    }
}

/// Percentage saved, from 0 to 100 (negative when the output grew). Zero for an empty input.
pub fn saved_percent(input_bytes: u64, output_bytes: u64) -> f64 {
    if input_bytes == 0 {
        return 0.0;
    }
    (input_bytes as f64 - output_bytes as f64) / input_bytes as f64 * 100.0
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub files: usize,
    pub optimized: usize,
    pub failed: usize,
    /// Input and output sizes of the optimized files only.
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub saved_percent: f64,
}

pub fn summarize(reports: &[FileReport]) -> Summary {
    let optimized: Vec<&FileReport> = reports
        .iter()
        .filter(|report| report.status == Status::Ok)
        .collect();
    let input_bytes: u64 = optimized.iter().filter_map(|r| r.input_bytes).sum();
    let output_bytes: u64 = optimized.iter().filter_map(|r| r.output_bytes).sum();

    Summary {
        files: reports.len(),
        optimized: optimized.len(),
        failed: reports.len() - optimized.len(),
        input_bytes,
        output_bytes,
        saved_percent: saved_percent(input_bytes, output_bytes),
    }
}

#[derive(Serialize)]
pub struct JsonReport<'a> {
    pub dry_run: bool,
    pub files: &'a [FileReport],
    pub summary: Summary,
}

pub fn json(reports: &[FileReport], dry_run: bool) -> String {
    let report = JsonReport {
        dry_run,
        files: reports,
        summary: summarize(reports),
    };
    serde_json::to_string_pretty(&report).expect("report contains only plain data")
}

/// Human readable lines: one per file, then a summary line.
pub fn human(reports: &[FileReport], dry_run: bool) -> String {
    let mut out = String::new();

    for report in reports {
        match (&report.status, &report.error) {
            (Status::Error, error) => {
                let message = error.as_deref().unwrap_or("unknown error");
                out.push_str(&format!("{}: error: {message}\n", report.input));
            }
            (Status::Ok, _) => {
                let input = report.input_bytes.unwrap_or(0);
                let output = report.output_bytes.unwrap_or(0);
                let percent = report.saved_percent.unwrap_or(0.0);
                out.push_str(&format!(
                    "{}: {input} -> {output} bytes ({})",
                    report.input,
                    describe_change(percent)
                ));
                // "-" means stdout, which is implied by the text that follows the report.
                if let Some(destination) = report.output.as_deref().filter(|d| *d != "-") {
                    out.push_str(&format!(" -> {destination}"));
                }
                out.push('\n');
            }
        }
    }

    let summary = summarize(reports);
    out.push_str(&format!(
        "{} optimized, {} failed: {} -> {} bytes ({})",
        summary.optimized,
        summary.failed,
        summary.input_bytes,
        summary.output_bytes,
        describe_change(summary.saved_percent)
    ));
    if dry_run {
        out.push_str(" [dry run, nothing written]");
    }
    out.push('\n');
    out
}

fn describe_change(percent: f64) -> String {
    if percent >= 0.0 {
        format!("{percent:.1}% smaller")
    } else {
        format!("{:.1}% larger", -percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_percent_is_relative_to_the_input() {
        assert_eq!(saved_percent(200, 150), 25.0);
        assert_eq!(saved_percent(100, 100), 0.0);
        assert_eq!(saved_percent(100, 0), 100.0);
    }

    #[test]
    fn growth_is_negative_and_empty_input_is_zero() {
        assert_eq!(saved_percent(100, 150), -50.0);
        assert_eq!(saved_percent(0, 10), 0.0);
    }

    #[test]
    fn optimized_report_records_bytes_and_savings() {
        let report = FileReport::optimized("a.svg".into(), Some("out/a.svg".into()), 1000, 750);
        assert_eq!(report.status, Status::Ok);
        assert_eq!(report.saved_bytes, Some(250));
        assert_eq!(report.saved_percent, Some(25.0));
        assert_eq!(report.error, None);
    }

    #[test]
    fn negative_savings_are_kept_as_negative_numbers() {
        let report = FileReport::optimized("a.svg".into(), None, 100, 130);
        assert_eq!(report.saved_bytes, Some(-30));
        assert_eq!(report.saved_percent, Some(-30.0));
    }

    #[test]
    fn summary_only_counts_optimized_files_in_byte_totals() {
        let reports = vec![
            FileReport::optimized("a.svg".into(), None, 1000, 500),
            FileReport::failed("bad.svg".into(), Some(9999), "not SVG".into()),
            FileReport::optimized("b.svg".into(), None, 1000, 1000),
        ];
        let summary = summarize(&reports);
        assert_eq!(summary.files, 3);
        assert_eq!(summary.optimized, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.input_bytes, 2000);
        assert_eq!(summary.output_bytes, 1500);
        assert_eq!(summary.saved_percent, 25.0);
    }

    #[test]
    fn summary_of_nothing_is_zero() {
        let summary = summarize(&[]);
        assert_eq!(
            (summary.files, summary.optimized, summary.failed),
            (0, 0, 0)
        );
        assert_eq!(summary.saved_percent, 0.0);
    }

    #[test]
    fn human_output_shows_each_file_and_the_totals() {
        let reports = vec![
            FileReport::optimized("a.svg".into(), Some("out/a.svg".into()), 1000, 750),
            FileReport::failed("b.svg".into(), Some(5), "could not parse".into()),
        ];
        let text = human(&reports, false);
        assert!(
            text.contains("a.svg: 1000 -> 750 bytes (25.0% smaller) -> out/a.svg"),
            "{text}"
        );
        assert!(text.contains("b.svg: error: could not parse"), "{text}");
        assert!(
            text.ends_with("1 optimized, 1 failed: 1000 -> 750 bytes (25.0% smaller)\n"),
            "{text}"
        );
    }

    #[test]
    fn human_output_says_when_the_file_grew_and_when_nothing_was_written() {
        let reports = vec![FileReport::optimized("a.svg".into(), None, 100, 110)];
        let text = human(&reports, true);
        assert!(text.contains("(10.0% larger)"), "{text}");
        assert!(!text.contains(" -> -"), "{text}");
        assert!(text.contains("[dry run, nothing written]"), "{text}");
    }

    #[test]
    fn json_has_files_summary_and_dry_run_flag() {
        let reports = vec![
            FileReport::optimized("a.svg".into(), Some("out/a.svg".into()), 100, 60),
            FileReport::failed("b.svg".into(), None, "bad \"quote\"\n".into()),
        ];
        let value: serde_json::Value = serde_json::from_str(&json(&reports, false)).unwrap();

        assert_eq!(value["dry_run"], false);
        assert_eq!(value["files"][0]["status"], "ok");
        assert_eq!(value["files"][0]["output"], "out/a.svg");
        assert_eq!(value["files"][0]["saved_bytes"], 40);
        assert_eq!(value["files"][1]["status"], "error");
        assert_eq!(value["files"][1]["output"], serde_json::Value::Null);
        assert_eq!(value["files"][1]["error"], "bad \"quote\"\n");
        assert_eq!(value["summary"]["failed"], 1);
        assert_eq!(value["summary"]["input_bytes"], 100);
        assert_eq!(value["summary"]["output_bytes"], 60);
    }

    #[test]
    fn json_marks_dry_runs() {
        let value: serde_json::Value = serde_json::from_str(&json(&[], true)).unwrap();
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["files"], serde_json::json!([]));
    }
}
