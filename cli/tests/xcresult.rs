use ctor::ctor;
use flate2::read::GzDecoder;
use std::fs::File;
use tar::Archive;
use trunk_analytics_cli::xcresult::XCResultFile;

#[cfg(test)]
#[ctor]
fn init() {
    let _ = std::fs::remove_dir_all("tests/data");
    let path = "tests/data.tar.gz";
    let file = File::open(path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let _ = archive.unpack("tests");
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_valid_path() {
    let path_str = "tests/data/test1.xcresult";
    let xcresult = XCResultFile::new(path_str.to_string());
    assert!(xcresult.is_ok());

    let junits = xcresult.unwrap().generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits[0].clone();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let expected_path = "tests/data/test1.junit";
    let expected = std::fs::read_to_string(expected_path).unwrap();
    assert_eq!(String::from_utf8(junit_writer).unwrap(), expected);
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_path() {
    let path_str = "tests/data/test2.xcresult";
    let xcresult = XCResultFile::new(path_str.to_string());
    assert!(xcresult.is_err());
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "failed to get absolute path"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_xcresult() {
    let path_str = "tests/data/test3.xcresult";
    let xcresult = XCResultFile::new(path_str.to_string());
    assert!(xcresult.is_err());
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "failed to parse json from xcrun output"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_complex_xcresult_with_valid_path() {
    let path_str = "tests/data/test4.xcresult";
    let xcresult = XCResultFile::new(path_str.to_string());
    assert!(xcresult.is_ok());

    let junits = xcresult.unwrap().generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits[0].clone();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let expected_path = "tests/data/test4.junit";
    let expected = std::fs::read_to_string(expected_path).unwrap();
    assert_eq!(String::from_utf8(junit_writer).unwrap(), expected);
}

#[cfg(target_os = "linux")]
#[test]
fn test_xcresult_with_valid_path_invalid_os() {
    let path_str = "tests/data/test1.xcresult";
    let xcresult = XCResultFile::new(path_str.to_string());
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "xcrun is only available on macOS"
    );
}
