//! Write an example monthly evidence CSV and, if given a path, a PDF.
//!
//! The counts are zeros. Replace them with a query before anyone treats
//! the file as an operational report.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use magpie::evidence::{self, MonthlyStats};

fn main() -> ExitCode {
	let stats = MonthlyStats::default();
	let rows = evidence::monthly_rows(&stats);
	let mut stderr = io::stderr();
	let _ = writeln!(
		stderr,
		"Example report. Counts are zero. Each row says whether the mapping is direct or interpretive."
	);
	print!("{}", evidence::to_csv(&rows));
	if let Some(path) = env::args().nth(1) {
		let pdf = evidence::to_pdf("Magpie Gateway example evidence report", &rows);
		if let Err(error) = fs::write(&path, pdf) {
			let _ = writeln!(stderr, "could not write {path}: {error}");
			return ExitCode::from(1);
		}
	}
	ExitCode::SUCCESS
}
