fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["proto/registry.proto"], &["proto"])?;
    Ok(())
}
