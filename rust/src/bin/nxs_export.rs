//! `nxs-export` — .nxb → JSON/CSV
//!
//! Flag contract: context/data/2026-04-30-converter-suite-spec.yaml § nxs_export

use clap::Parser;
use nxs::convert::{self, BinaryEncoding, CommonOpts, ExportArgs, ExportFormat};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "nxs-export", about = "Export .nxb to JSON or CSV")]
struct Cli {
    /// Target format (required)
    #[arg(long, value_name = "json|csv")]
    to: String,

    /// (JSON only) Pretty-print with 2-space indent
    #[arg(long)]
    pretty: bool,

    /// (JSON only) Newline-delimited JSON output
    #[arg(long)]
    ndjson: bool,

    /// (CSV only) Explicit column list, comma-separated
    #[arg(long, value_name = "a,b,c")]
    columns: Option<String>,

    /// CSV field delimiter (default: comma)
    #[arg(long, value_name = "CHAR")]
    csv_delimiter: Option<char>,

    /// How to encode binary (`<`) values
    #[arg(long, value_name = "base64|hex|skip", default_value = "base64")]
    binary: String,

    /// (CSV only) Prefix injection-prone cell values with `'`
    #[arg(long)]
    csv_safe: bool,

    /// Input .nxb file (`-` for stdin)
    input: String,

    /// Output file (`-` for stdout)
    output: Option<String>,
}

fn parse_export_format(s: &str) -> Result<ExportFormat, String> {
    match s {
        "json" => Ok(ExportFormat::Json),
        "csv" => Ok(ExportFormat::Csv),
        other => Err(format!("unknown format '{other}'; expected json or csv")),
    }
}

fn parse_binary_encoding(s: &str) -> Result<BinaryEncoding, String> {
    match s {
        "base64" => Ok(BinaryEncoding::Base64),
        "hex" => Ok(BinaryEncoding::Hex),
        "skip" => Ok(BinaryEncoding::Skip),
        other => Err(format!(
            "unknown --binary '{other}'; expected base64, hex, or skip"
        )),
    }
}

fn main() {
    let cli = Cli::parse();

    let to = parse_export_format(&cli.to).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
    let binary = parse_binary_encoding(&cli.binary).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
    let columns = cli
        .columns
        .as_deref()
        .map(|s| s.split(',').map(str::to_owned).collect::<Vec<_>>());

    let input_path = if cli.input == "-" {
        None
    } else {
        Some(PathBuf::from(&cli.input))
    };
    let output_path = cli.output.as_deref().and_then(|s| {
        if s == "-" {
            None
        } else {
            Some(PathBuf::from(s))
        }
    });

    let args = ExportArgs {
        common: CommonOpts {
            input_path,
            output_path,
        },
        to,
        pretty: cli.pretty,
        ndjson: cli.ndjson,
        columns,
        csv_delimiter: cli.csv_delimiter,
        binary,
        csv_safe: cli.csv_safe,
    };

    match convert::run_export(&args) {
        Ok(report) => {
            eprintln!(
                "exported {} records ({} B)",
                report.records_read, report.output_bytes
            );
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(convert::exit_code_for(&e));
        }
    }
}
