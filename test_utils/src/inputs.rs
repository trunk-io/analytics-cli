use std::path::PathBuf;

pub fn get_test_file_path(file: &str) -> String {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join(file)
        .to_str()
        .unwrap()
        .to_string()
}
