use crate::scanner::{BundleRepo, FileSet, FileSetCounter};
use std::process::Stdio;
use crate::types::{RunResult, Test};
use crate::constants::EXIT_FAILURE;
use junit_parser;
use std::process::Command;

pub async fn run_test_command(
    repo: &BundleRepo,
    command: &String,
    args: Vec<&String>,
    output_paths: Vec<&String>,
) -> anyhow::Result<RunResult> {
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
                let file = std::fs::File::open(&file.original_path)?;
                let reader = std::io::BufReader::new(file);
                let junitxml = junit_parser::from_reader(reader)?;
                for suite in junitxml.suites {
                    for case in suite.cases {
                        let failure = case.status.is_failure();
                        if failure {
                            failures.push(Test {
                                parent_name: suite.name.clone(),
                                name: case.name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    let exit_code = result.code().unwrap_or(EXIT_FAILURE);
    return Ok(RunResult {
        exit_code: exit_code,
        failures: failures,
    });
}
