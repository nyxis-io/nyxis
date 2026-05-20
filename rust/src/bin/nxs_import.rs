//! `nxs-import` — JSON/CSV/XML → .nxb
//!
//! Flag contract: context/data/2026-04-30-converter-suite-spec.yaml § nxs_import

use clap::Parser;
use nxs::convert::{
    self, CommonOpts, ConflictPolicy, ImportArgs, ImportFormat, VerifyPolicy, XmlAttrsMode,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "nxs-import", about = "Import JSON/CSV/XML into .nxb")]
struct Cli {
    /// Source format (required)
    #[arg(long, value_name = "json|csv|xml")]
    from: String,

    /// Schema hint file (skips inference; enables single-pass)
    #[arg(long, value_name = "FILE")]
    schema: Option<PathBuf>,

    /// Conflict resolution policy
    #[arg(
        long,
        value_name = "error|coerce-string|first-wins",
        default_value = "error"
    )]
    on_conflict: String,

    /// JSON path to the record array (JSON only)
    #[arg(long, value_name = "JSONPATH")]
    root: Option<String>,

    /// CSV field delimiter (default: comma)
    #[arg(long, value_name = "CHAR")]
    csv_delimiter: Option<char>,

    /// Treat first row as data, not header; use positional keys col_0, col_1, …
    #[arg(long)]
    csv_no_header: bool,

    /// XML element name that delimits one record (XML required)
    #[arg(long, value_name = "NAME")]
    xml_record_tag: Option<String>,

    /// How to handle XML attributes
    #[arg(long, value_name = "as-fields|prefix", default_value = "as-fields")]
    xml_attrs: String,

    /// Records buffered before flushing to writer
    #[arg(long, value_name = "N", default_value = "4096")]
    buffer_records: usize,

    /// Maximum JSON/XML nesting depth
    #[arg(long, value_name = "N", default_value = "64")]
    max_depth: usize,

    /// XML-specific nesting depth cap (effective = min(max_depth, xml_max_depth))
    #[arg(long, value_name = "N", default_value = "64")]
    xml_max_depth: usize,

    /// Allow tail-index to spill to disk when over 512 MB
    #[arg(long)]
    tail_index_spill: bool,

    /// Roundtrip verify policy
    #[arg(long, value_name = "auto|force|off", default_value = "auto")]
    verify: String,

    /// Input file (`-` for stdin)
    input: String,

    /// Output file (`-` for stdout; default: <input>.nxb)
    output: Option<String>,
}

fn parse_import_format(s: &str) -> Result<ImportFormat, String> {
    match s {
        "json" => Ok(ImportFormat::Json),
        "csv" => Ok(ImportFormat::Csv),
        "xml" => Ok(ImportFormat::Xml),
        other => Err(format!(
            "unknown format '{other}'; expected json, csv, or xml"
        )),
    }
}

fn parse_conflict(s: &str) -> Result<ConflictPolicy, String> {
    match s {
        "error" => Ok(ConflictPolicy::Error),
        "coerce-string" => Ok(ConflictPolicy::CoerceString),
        "first-wins" => Ok(ConflictPolicy::FirstWins),
        other => Err(format!(
            "unknown --on-conflict '{other}'; expected error, coerce-string, or first-wins"
        )),
    }
}

fn parse_verify(s: &str) -> Result<VerifyPolicy, String> {
    match s {
        "auto" => Ok(VerifyPolicy::Auto),
        "force" => Ok(VerifyPolicy::Force),
        "off" => Ok(VerifyPolicy::Off),
        other => Err(format!(
            "unknown --verify '{other}'; expected auto, force, or off"
        )),
    }
}

fn parse_xml_attrs(s: &str) -> Result<XmlAttrsMode, String> {
    match s {
        "as-fields" => Ok(XmlAttrsMode::AsFields),
        "prefix" => Ok(XmlAttrsMode::Prefix),
        other => Err(format!(
            "unknown --xml-attrs '{other}'; expected as-fields or prefix"
        )),
    }
}

/// Derive output path from input using only `file_name()` — never traverses `..`.
fn derive_output_path(input: &str, explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(out) = explicit {
        if out == "-" {
            return None; // stdout
        }
        return Some(PathBuf::from(out));
    }
    if input == "-" {
        return None; // stdin→stdout by default
    }
    let p = std::path::Path::new(input);
    let stem = p
        .file_name()
        .and_then(|n| std::path::Path::new(n).file_stem())?;
    Some(PathBuf::from(stem).with_extension("nxb"))
}

fn main() {
    let cli = Cli::parse();

    let from = parse_import_format(&cli.from).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
    let conflict = parse_conflict(&cli.on_conflict).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
    let verify = parse_verify(&cli.verify).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });
    let xml_attrs = parse_xml_attrs(&cli.xml_attrs).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(2);
    });

    let input_path = if cli.input == "-" {
        None
    } else {
        Some(PathBuf::from(&cli.input))
    };
    let output_path = derive_output_path(&cli.input, cli.output.as_deref());

    let args = ImportArgs {
        common: CommonOpts {
            input_path,
            output_path,
        },
        from,
        schema_hint: cli.schema,
        conflict,
        root: cli.root,
        csv_delimiter: cli.csv_delimiter,
        csv_no_header: cli.csv_no_header,
        xml_record_tag: cli.xml_record_tag,
        xml_attrs,
        buffer_records: cli.buffer_records,
        max_depth: cli.max_depth,
        xml_max_depth: cli.xml_max_depth,
        tail_index_spill: cli.tail_index_spill,
        verify,
    };

    match convert::run_import(&args) {
        Ok(report) => {
            let out = args
                .common
                .output_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<stdout>".into());
            eprintln!(
                "imported {} records → {} ({} B)",
                report.records_written, out, report.output_bytes
            );
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(convert::exit_code_for(&e));
        }
    }
}
