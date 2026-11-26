//! Command-line entry for `monarch-synth`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    default_coverage_path, default_csv_path, generate, render_coverage, render_csv, DEFAULT_SEED,
};

/// Run the generator the way the `monarch-synth` binary does.
///
/// Returns `0` when the write or check succeeds, and `1` when `--check` finds
/// a difference. Usage and I/O problems are errors.
pub fn execute<I, S>(args: I) -> Result<i32, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seed = DEFAULT_SEED;
    let mut out = default_csv_path();
    let mut coverage = default_coverage_path();
    let mut check = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "--help" | "-h" => {
                print_help();
                return Ok(0);
            }
            "--check" => check = true,
            "--seed" => {
                let value = next_value(&mut args, "--seed")?;
                seed = parse_seed(&value)?;
            }
            "--out" => out = PathBuf::from(next_value(&mut args, "--out")?),
            "--coverage" => coverage = PathBuf::from(next_value(&mut args, "--coverage")?),
            other => return Err(format!("unknown argument {other} (try --help)")),
        }
    }

    let fixture = generate(seed);
    let csv = render_csv(&fixture);
    let markdown = render_coverage(&fixture);

    if check {
        let csv_ok = files_match(&out, &csv)?;
        let coverage_ok = files_match(&coverage, &markdown)?;
        if csv_ok && coverage_ok {
            println!("fixture matches seed {seed}");
            return Ok(0);
        }
        if !csv_ok {
            eprintln!("{} differs from seed {seed}", out.display());
        }
        if !coverage_ok {
            eprintln!("{} differs from seed {seed}", coverage.display());
        }
        return Ok(1);
    }

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    if let Some(parent) = coverage.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
    }
    fs::write(&out, csv).map_err(|err| err.to_string())?;
    fs::write(&coverage, markdown).map_err(|err| err.to_string())?;
    println!(
        "wrote {} ({} rows) and {}",
        out.display(),
        fixture.rows.len(),
        coverage.display()
    );
    Ok(0)
}

fn print_help() {
    println!(
        "\
monarch-synth writes a synthetic October 2025 Monarch transactions CSV.

Usage:
  monarch-synth [--seed <u64>] [--out <csv>] [--coverage <md>]
  monarch-synth --check [--seed <u64>] [--out <csv>] [--coverage <md>]

The default seed is {DEFAULT_SEED}. One seed always produces the same bytes.
Scenario row ids stay fixed; the seed changes filler merchants, amounts, and days.
--check exits 0 when the CSV and coverage files already match that seed.
"
    );
}

fn next_value<I, S>(args: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    args.next()
        .map(|value| value.as_ref().to_string())
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_seed(raw: &str) -> Result<u64, String> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|_| format!("invalid hex seed {raw}"))
    } else {
        raw.parse::<u64>()
            .map_err(|_| format!("invalid seed {raw}"))
    }
}

fn files_match(path: &Path, expected: &str) -> Result<bool, String> {
    match fs::read_to_string(path) {
        Ok(found) => Ok(found == expected),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.to_string()),
    }
}
