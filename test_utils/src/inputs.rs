use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;

pub fn get_test_file_path(file: &str) -> String {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join(file)
        .to_str()
        .unwrap()
        .to_string()
}

pub fn unpack_archive_to_dir<T: AsRef<Path>>(archive_file_path: T, directory: T) -> PathBuf {
    let file = File::open(archive_file_path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    if let Err(e) = archive.unpack(directory.as_ref()) {
        panic!("failed to unpack data.tar.gz: {}", e);
    }
    directory.as_ref().to_path_buf()
}
