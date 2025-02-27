// Include the `trunk` module, which is generated from upload_metrics.proto.
// It is important to maintain the same structure as in the proto.
pub mod trunk {
    include!(concat!(env!("OUT_DIR"), "/trunk.oss.flakytests_cli.v1.rs"));
}
