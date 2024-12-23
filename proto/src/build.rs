use std::io::Result;

fn main() -> Result<()> {
    let mut prost_build = prost_build::Config::new();
    prost_build
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_wkt_types::Timestamp")
        .compile_protos(&["proto/test_context.proto"], &["proto/"])?;
    Ok(())
}
