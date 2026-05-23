//! `nxs` CLI — compile `.nxs` → `.nxb` and registry operations against `nxs-registryd`.

mod registry;

use clap::{Parser, Subcommand};
use nxs::error::NxsError;
use nxs::layout::{CompileOptions, Layout};
use registry::client::RegistryClient;
use registry::preamble::extract_preamble;
use std::path::{Path, PathBuf};

const DEFAULT_REGISTRY_SERVER: &str = "127.0.0.1:7946";

#[derive(Parser)]
#[command(name = "nxs", about = "Nyxis compiler and schema registry client")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile `.nxs` source to `.nxb` binary
    Compile(CompileArgs),
    /// Schema registry (gRPC `nyxis.registry.v1` — requires `nxs-registryd`)
    Registry(RegistryArgs),
}

#[derive(Parser)]
struct CompileArgs {
    #[arg(long, value_name = "row|columnar|pax", default_value = "row")]
    layout: String,
    #[arg(long, value_name = "N")]
    page_size: Option<u32>,
    /// Input `.nxs` file
    input: PathBuf,
    /// Output `.nxb` file (default: same basename with `.nxb`)
    #[arg(value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

#[derive(Parser)]
struct RegistryArgs {
    #[command(subcommand)]
    command: RegistryCommands,
}

#[derive(Subcommand)]
enum RegistryCommands {
    /// Register schema from `.nxb` (or compile `.nxs` first)
    Push(PushArgs),
    /// List schemas via `ListSchemas`, or probe by DictHash with `--hash`
    List(ListArgs),
    /// Compare two DictHashes or `.nxb` files (local and/or via registry)
    Diff(DiffArgs),
}

#[derive(Parser)]
struct PushArgs {
    /// Registry gRPC host:port
    #[arg(long, default_value = DEFAULT_REGISTRY_SERVER)]
    server: String,
    /// Drift policy: reject | additive_only | proxy_rewrite
    #[arg(long, default_value = "additive_only")]
    drift_policy: String,
    /// `.nxs` or `.nxb` file
    file: PathBuf,
}

#[derive(Parser)]
struct ListArgs {
    #[arg(long, default_value = DEFAULT_REGISTRY_SERVER)]
    server: String,
    #[arg(long, default_value = "1000")]
    limit: u32,
    #[arg(long, default_value = "0")]
    offset: u32,
    /// DictHash to probe (repeatable). Skips `ListSchemas` when any hash is given.
    #[arg(long = "hash", value_name = "HEX")]
    hashes: Vec<String>,
}

#[derive(Parser)]
struct DiffArgs {
    #[arg(long, default_value = DEFAULT_REGISTRY_SERVER)]
    server: String,
    /// First operand: 16-hex DictHash or `.nxb` path
    a: String,
    /// Second operand: 16-hex DictHash or `.nxb` path
    b: String,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Compile(args) => run_compile(args),
        Commands::Registry(args) => run_registry(args),
    }
}

fn run_compile(args: CompileArgs) {
    let layout = Layout::parse_name(&args.layout).unwrap_or_else(|| {
        eprintln!("error: unknown layout (row|columnar|pax)");
        std::process::exit(1);
    });
    let page_size = match args.page_size {
        Some(n) => n,
        None if layout == Layout::Pax => 4096,
        None => 0,
    };
    let opts = CompileOptions { layout, page_size };

    let output_path = args.output.clone().unwrap_or_else(|| {
        args.input
            .with_extension("nxb")
            .to_string_lossy()
            .into_owned()
            .into()
    });

    let source = std::fs::read_to_string(&args.input)
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
        args.input.display(),
        output_path.display(),
        binary.len(),
        opts.layout
    );
}

fn run_registry(args: RegistryArgs) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("error: tokio runtime: {e}");
            std::process::exit(1);
        });
    if let Err(msg) = rt.block_on(async { dispatch_registry(args).await }) {
        eprintln!("error: {msg}");
        std::process::exit(1);
    }
}

async fn dispatch_registry(args: RegistryArgs) -> Result<(), String> {
    match args.command {
        RegistryCommands::Push(p) => registry_push(p).await,
        RegistryCommands::List(l) => registry_list(l).await,
        RegistryCommands::Diff(d) => registry_diff(d).await,
    }
}

async fn registry_push(args: PushArgs) -> Result<(), String> {
    let nxb = load_nxb_bytes(&args.file)?;
    let preamble = extract_preamble(&nxb).map_err(|e| e.join(", "))?;
    let dict_hash = preamble.dict_hash.to_le_bytes();

    let mut client = RegistryClient::connect(&args.server).await?;
    let resp = client
        .register_schema(dict_hash, preamble.schema_bytes, &args.drift_policy)
        .await?;

    println!(
        "registered {} dict_hash={} version={}",
        args.file.display(),
        registry::format_dict_hash(&dict_hash),
        resp.version
    );
    Ok(())
}

async fn registry_list(args: ListArgs) -> Result<(), String> {
    let mut client = RegistryClient::connect(&args.server).await?;

    if !args.hashes.is_empty() {
        println!("dict_hash\tversion\tdrift_policy\tschema_bytes");
        for h in &args.hashes {
            let hash = registry::parse_dict_hash_hex(h)?;
            match client.get_schema_by_hash(hash).await {
                Ok(row) => println!(
                    "{}\t{}\t{}\t{}",
                    registry::format_dict_hash(&hash),
                    row.version,
                    row.drift_policy,
                    row.schema_bytes.len()
                ),
                Err(e) => eprintln!("{}: {e}", registry::format_dict_hash(&hash)),
            }
        }
        return Ok(());
    }

    let resp = client.list_schemas(args.limit, args.offset).await?;
    if resp.schemas.is_empty() && args.offset == 0 {
        println!("(no schemas)");
        return Ok(());
    }
    println!("dict_hash\tversion\tdrift_policy\tkey_count");
    for entry in &resp.schemas {
        let hash = dict_hash_from_proto(&entry.dict_hash)?;
        println!(
            "{}\t{}\t{}\t{}",
            registry::format_dict_hash(&hash),
            entry.version,
            entry.drift_policy,
            entry.key_count
        );
    }
    let shown = resp.schemas.len() as u32;
    if args.offset + shown < resp.total {
        println!(
            "# showing {}–{} of {}",
            args.offset + 1,
            args.offset + shown,
            resp.total
        );
    }
    Ok(())
}

fn dict_hash_from_proto(bytes: &[u8]) -> Result<[u8; 8], String> {
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| format!("dict_hash must be 8 bytes (got {})", bytes.len()))?;
    Ok(arr)
}

async fn registry_diff(args: DiffArgs) -> Result<(), String> {
    let mut client = RegistryClient::connect(&args.server).await?;
    let (hash_a, schema_a, src_a) = resolve_operand(&mut client, &args.a).await?;
    let (hash_b, schema_b, src_b) = resolve_operand(&mut client, &args.b).await?;

    println!("a: {} ({})", registry::format_dict_hash(&hash_a), src_a);
    println!("b: {} ({})", registry::format_dict_hash(&hash_b), src_b);

    if hash_a != hash_b {
        println!("dict_hash: DIFFER");
    } else {
        println!("dict_hash: same");
    }

    if schema_a == schema_b {
        println!("schema_bytes: same ({} bytes)", schema_a.len());
    } else {
        println!(
            "schema_bytes: DIFFER ({} vs {} bytes)",
            schema_a.len(),
            schema_b.len()
        );
    }
    Ok(())
}

async fn resolve_operand(
    client: &mut RegistryClient,
    arg: &str,
) -> Result<([u8; 8], Vec<u8>, String), String> {
    let path = Path::new(arg);
    if path.exists() {
        let nxb = load_nxb_bytes(path)?;
        let p = extract_preamble(&nxb).map_err(|e| e.join(", "))?;
        return Ok((
            p.dict_hash.to_le_bytes(),
            p.schema_bytes,
            path.display().to_string(),
        ));
    }
    let hash = registry::parse_dict_hash_hex(arg)?;
    let row = client.get_schema_by_hash(hash).await?;
    Ok((hash, row.schema_bytes, format!("registry:{}", arg)))
}

fn load_nxb_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "nxs" {
        let source =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        nxs::compile_source(&source).map_err(|e| format!("compile {}: {e}", path.display()))
    } else {
        std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))
    }
}
