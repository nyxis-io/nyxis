//! `nxs` CLI — compile `.nxs` → `.nxb` with optional columnar / PAX layout.

use nxs::error::NxsError;
use nxs::layout::{CompileOptions, Layout};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: nxs compile [--layout row|columnar|pax] [--page-size N] <input.nxs> [output.nxb]"
        );
        std::process::exit(1);
    }

    let (opts, rest) = parse_cli(&args[1..]);
    if rest.is_empty() {
        eprintln!("error: missing input file");
        std::process::exit(1);
    }

    let input_path = Path::new(&rest[0]);
    let output_path = if rest.len() >= 2 {
        rest[1].clone()
    } else {
        input_path
            .with_extension("nxb")
            .to_string_lossy()
            .to_string()
    };

    let source = std::fs::read_to_string(input_path)
        .map_err(|e| NxsError::IoError(e.to_string()))
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });

    let binary = nxs::compile_source_with_opts(&source, &opts).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    std::fs::write(&output_path, &binary)
        .map_err(|e| NxsError::IoError(e.to_string()))
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });

    println!(
        "compiled {} → {} ({} bytes, layout={:?})",
        input_path.display(),
        output_path,
        binary.len(),
        opts.layout
    );
}

fn parse_cli(args: &[String]) -> (CompileOptions, Vec<String>) {
    let mut opts = CompileOptions::default();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "compile" => {
                i += 1;
            }
            "--layout" => {
                i += 1;
                if i < args.len() {
                    opts.layout = Layout::from_str(&args[i]).unwrap_or_else(|| {
                        eprintln!("error: unknown layout (row|columnar|pax)");
                        std::process::exit(1);
                    });
                }
                i += 1;
            }
            "--page-size" => {
                i += 1;
                if i < args.len() {
                    opts.page_size = args[i].parse().unwrap_or_else(|_| {
                        eprintln!("error: bad --page-size");
                        std::process::exit(1);
                    });
                }
                i += 1;
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown flag {other}");
                std::process::exit(1);
            }
            _ => {
                rest.push(args[i].clone());
                i += 1;
            }
        }
    }
    if opts.page_size == 0 && opts.layout == Layout::Pax {
        opts.page_size = 4096;
    }
    (opts, rest)
}
