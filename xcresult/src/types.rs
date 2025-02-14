#[allow(dead_code, clippy::all)]
pub mod schema {
    include!(concat!(
        env!("OUT_DIR"),
        "/xcrun-xcresulttool-get-test-results-tests-json-schema.rs"
    ));
}

#[allow(dead_code, clippy::all)]
pub mod legacy_schema {
    include!(concat!(
        env!("OUT_DIR"),
        "/xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.rs"
    ));
}
