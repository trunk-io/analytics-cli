use std::{env, io, path::PathBuf};

fn main() -> io::Result<()> {
    let protos = std::fs::read_dir("proto")?
        .filter(|a| {
            if let Ok(a) = a {
                a.file_type().unwrap().is_file()
            } else {
                false
            }
        })
        .map(|a| a.map(|a| a.path().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;

    let descriptor_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("proto_descriptor.bin");

    let compiler = tonic_build::configure();
    #[cfg(not(feature = "client"))]
    let compiler = compiler.build_client(false);
    #[cfg(not(feature = "server"))]
    let compiler = compiler.build_server(false);

    compiler
        .file_descriptor_set_path(&descriptor_path)
        .compile_well_known_types(true)
        // Override prost-types with pbjson-types
        .extern_path(".google.protobuf", "::pbjson_types")
        .compile(&protos, &["proto/"])?;

    let descriptor_set = std::fs::read(descriptor_path)?;
    pbjson_build::Builder::new()
        .register_descriptors(&descriptor_set)?
        .build(&["."])?;

    Ok(())
}
