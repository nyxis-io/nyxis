fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new()
        .compile_protos(&["../../schemas/flat8.proto"], &["../../schemas/"])?;
    capnpc::CompilerCommand::new()
        .src_prefix("../../schemas")
        .file("../../schemas/flat8.capnp")
        .run()?;
    Ok(())
}
