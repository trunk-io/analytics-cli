fn main() {
    let file_descriptors = protox::compile(
        [
            "proto/test_context.proto",
            "proto/common.proto",
            "proto/upload_metrics.proto",
        ],
        ["proto/"],
    )
    .unwrap();
    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .extern_path(".google.protobuf.Timestamp", "::prost_wkt_types::Timestamp")
        .compile_fds(file_descriptors)
        .unwrap();
}
