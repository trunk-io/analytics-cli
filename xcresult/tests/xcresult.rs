use ctor::ctor;
use flate2::read::GzDecoder;
use lazy_static::lazy_static;
use std::fs::File;
use tar::Archive;
use temp_testdir::TempDir;
use xcresult::XCResult;

lazy_static! {
    static ref TEMP_DIR: TempDir = TempDir::default();
}

#[cfg(test)]
#[ctor]
fn init() {
    let path = "tests/data.tar.gz";
    let file = File::open(path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    match archive.unpack(TEMP_DIR.as_ref()) {
        Ok(_) => (),
        Err(e) => panic!("failed to unpack data.tar.gz: {}", e),
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_valid_path() {
    let path = TEMP_DIR.as_ref().join("data/test1.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str);
    assert!(xcresult.is_ok());

    let junits = xcresult.unwrap().generate_junits().unwrap();
    assert_eq!(junits.len(), 1);
    let junit = junits[0].clone();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let expected_path = TEMP_DIR.as_ref().join("data/test1.junit");
    let expected = std::fs::read_to_string(expected_path).unwrap();
    assert_eq!(String::from_utf8(junit_writer).unwrap(), expected);
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_path() {
    let path = TEMP_DIR.as_ref().join("data/test2.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str);
    assert!(xcresult.is_err());
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "failed to get absolute path -- is the path correct?"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_xcresult() {
    let path = TEMP_DIR.as_ref().join("data/test3.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str);
    assert!(xcresult.is_err());
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "failed to parse json from xcrun output"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_complex_xcresult_with_valid_path() {
    let path = TEMP_DIR.as_ref().join("data/test4.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str);
    assert!(xcresult.is_ok());

    let junits = xcresult.unwrap().generate_junits().unwrap();
    assert_eq!(junits.len(), 1);
    let junit = junits[0].clone();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let expected_path = TEMP_DIR.as_ref().join("data/test4.junit");
    let expected = std::fs::read_to_string(expected_path).unwrap();
    assert_eq!(String::from_utf8(junit_writer).unwrap(), expected);
}

#[cfg(target_os = "linux")]
#[test]
fn test_xcresult_with_valid_path_invalid_os() {
    let path = TEMP_DIR.as_ref().join("data/test1.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str);
    assert_eq!(
        xcresult.err().unwrap().to_string(),
        "xcrun is only available on macOS"
    );
}