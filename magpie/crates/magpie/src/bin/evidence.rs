//! Write a monthly evidence CSV and, optionally, a PDF.
//!
//! Pass `--db <path-to-magpie.sqlite>` to populate live counts from `audit_events`.
//! Without `--db`, counts default to zero (example operational report).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use magpie::evidence::{self, MonthlyStats};
use magpie::store::Store;

fn main() -> ExitCode {
	let mut args = env::args().skip(1);
	let mut db_path: Option<String> = None;
	let mut pdf_path: Option<String> = None;
	while let Some(arg) = args.next() {
		if arg == "--db" {
			db_path = args.next();
		} else if pdf_path.is_none() {
			pdf_path = Some(arg);
		}
	}
	let mut stderr = io::stderr();
	let (stats, title) = match db_path {
		Some(path) => match Store::open_path(&path).and_then(|s| s.monthly_stats(None, None)) {
			Ok(stats) => (stats, "Magpie Gateway operational evidence report"),
			Err(error) => {
				let _ = writeln!(stderr, "could not query {path}: {error}");
				return ExitCode::from(1);
			},
		},
		None => {
			let _ = writeln!(
				stderr,
				"Example report. Counts are zero. Pass --db <magpie.sqlite> for live counts. Each row says whether the mapping is direct or interpretive."
			);
			(
				MonthlyStats::default(),
				"Magpie Gateway example evidence report",
			)
		},
	};
	let rows = evidence::monthly_rows(&stats);
	print!("{}", evidence::to_csv(&rows));
	if let Some(path) = pdf_path {
		let pdf = evidence::to_pdf(title, &rows);
		if let Err(error) = fs::write(&path, pdf) {
			let _ = writeln!(stderr, "could not write {path}: {error}");
			return ExitCode::from(1);
		}
	}
	ExitCode::SUCCESS
}
