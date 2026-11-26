//! Write the synthetic October 2025 Monarch CSV and its scenario coverage table.

use std::process::ExitCode;

fn main() -> ExitCode {
    match monarch_synth::execute(std::env::args().skip(1)) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(err) => {
            eprintln!("monarch-synth: {err}");
            ExitCode::from(1)
        }
    }
}
