#[cfg(target_os = "macos")]
use context::repo::BundleRepo;
#[cfg(target_os = "macos")]
use context::repo::RepoUrlParts;
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use xcresult::XCResult;

pub fn junit_require() -> &'static str {
    if cfg!(target_os = "macos") {
        "xcresult_path"
    } else {
        "junit_paths"
    }
}

#[cfg(target_os = "macos")]
pub fn junitify_xcresult(
    xcresult_path: &Option<String>,
    base_junit_paths: &Vec<String>,
    repo: &BundleRepo,
    org_url_slug: &String,
    allow_empty_test_results: &bool,
) -> anyhow::Result<Vec<String>> {
    let junit_temp_dir = tempfile::tempdir()?;
    let temp_paths = handle_xcresult(&junit_temp_dir, xcresult_path, &repo.repo, &org_url_slug)?;
    let junit_paths = [base_junit_paths.as_slice(), temp_paths.as_slice()].concat();
    if junit_paths.is_empty() && !allow_empty_test_results {
        return Err(anyhow::anyhow!(
            "No tests found in the provided XCResult path."
        ));
    } else if junit_paths.is_empty() && allow_empty_test_results {
        log::warn!("No tests found in the provided XCResult path.");
    }
    Ok(junit_paths)
}

#[cfg(target_os = "macos")]
fn handle_xcresult(
    junit_temp_dir: &tempfile::TempDir,
    xcresult_path: Option<String>,
    repo: &RepoUrlParts,
    org_url_slug: &str,
) -> Result<Vec<String>, anyhow::Error> {
    let mut temp_paths = Vec::new();
    if let Some(xcresult_path) = xcresult_path {
        let xcresult = XCResult::new(xcresult_path, repo, org_url_slug.to_string());
        let junits = xcresult?
            .generate_junits()
            .map_err(|e| anyhow::anyhow!("Failed to generate junit files from xcresult: {}", e))?;
        for (i, junit) in junits.iter().enumerate() {
            let mut junit_writer: Vec<u8> = Vec::new();
            junit.serialize(&mut junit_writer)?;
            let junit_temp_path = junit_temp_dir
                .path()
                .join(format!("xcresult_junit_{}.xml", i));
            let mut junit_temp = std::fs::File::create(&junit_temp_path)?;
            junit_temp
                .write_all(&junit_writer)
                .map_err(|e| anyhow::anyhow!("Failed to write junit file: {}", e))?;
            let junit_temp_path_str = junit_temp_path.to_str();
            if let Some(junit_temp_path_string) = junit_temp_path_str {
                temp_paths.push(junit_temp_path_string.to_string());
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to convert junit temp path to string."
                ));
            }
        }
    }
    Ok(temp_paths)
}
