use crate::constants::EXIT_FAILURE;
use crate::scanner::{BundleRepo, FileSet, FileSetCounter};
use crate::types::{RunResult, Test};
use junit_parser;
use std::fs::metadata;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;

pub async fn run_test_command(
    repo: &BundleRepo,
    command: &String,
    args: Vec<&String>,
    output_paths: Vec<&String>,
) -> anyhow::Result<RunResult> {
    let start = SystemTime::now();
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let result = child.wait()?;
    let mut failures = Vec::<Test>::new();
    if !result.success() {
        let mut file_counter = FileSetCounter::default();
        let file_sets = output_paths
            .iter()
            .map(|path| {
                FileSet::scan_from_glob(&repo.repo_root, path.to_string(), &mut file_counter)
            })
            .collect::<anyhow::Result<Vec<FileSet>>>()?;
        for file_set in &file_sets {
            for file in &file_set.files {
                log::debug!("Checking file: {}", file.original_path);
                let metadata = metadata(&file.original_path)?;
                let time = metadata.modified()?;
                // skip files that were last modified before the test started
                if time <= start {
                    log::debug!(
                        "Skipping file because of lack of modification: {}",
                        file.original_path
                    );
                    continue;
                }
                let file = std::fs::File::open(&file.original_path)?;
                let reader = std::io::BufReader::new(file);
                let junitxml = junit_parser::from_reader(reader)?;
                for suite in junitxml.suites {
                    let parent_name = if junitxml.name.is_empty() {
                        suite.name
                    } else {
                        format!("{}/{}", junitxml.name, suite.name)
                    };
                    for case in suite.cases {
                        let failure = case.status.is_failure();
                        if failure {
                            let name = case.original_name;
                            log::debug!("Test failed: {} -> {}", parent_name, name);
                            failures.push(Test {
                                parent_name: parent_name.clone(),
                                name: name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    let exit_code = result.code().unwrap_or(EXIT_FAILURE);
    Ok(RunResult {
        exit_code,
        failures,
    })
}
