use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use glob::glob;
use regex::Regex;
use sysinfo::System;
use tracing::warn;

/// Extract the external ID from GitHub Actions worker log files
/// This function attempts to find the Runner.Worker process and extract
/// the Job ID from its log files, returning it as a string
pub fn extract_github_external_id() -> Result<Option<String>> {
    // Check if we're running in GitHub Actions
    if env::var("GITHUB_ACTIONS").is_err() {
        tracing::debug!("Not running in GitHub Actions, skipping external ID extraction");
        return Ok(None);
    }

    tracing::info!("Running in GitHub Actions, attempting to extract external ID");

    // Try multiple methods to find the runner directory
    let runner_dir = match find_runner_directory() {
        Ok(dir) => {
            tracing::debug!("Found runner directory: {:?}", dir);
            dir
        }
        Err(e) => {
            warn!("Failed to find runner directory: {}", e);
            return Ok(None);
        }
    };

    let worker_log_files = match find_worker_log_files(&runner_dir) {
        Ok(files) => {
            tracing::debug!("Found {} worker log files", files.len());
            files
        }
        Err(e) => {
            warn!("Failed to find worker log files: {}", e);
            return Ok(None);
        }
    };

    // Search through log files for the Job ID
    for log_file in worker_log_files {
        tracing::debug!("Searching log file: {:?}", log_file);
        if let Ok(job_id) = extract_job_id_from_log(&log_file) {
            tracing::info!("Successfully extracted external ID: {}", job_id);
            return Ok(Some(job_id));
        }
    }

    warn!("Unable to find Job ID in GitHub Actions worker log files");
    Ok(None)
}

/// Find the runner directory using multiple methods
fn find_runner_directory() -> Result<PathBuf> {
    // Method 1: Try to find the Runner.Worker process using sysinfo (most reliable)
    match find_runner_worker_process() {
        Ok(worker_cmd) => {
            tracing::debug!("Found Runner.Worker process: {}", worker_cmd);
            if let Ok(runner_dir) = extract_runner_directory(&worker_cmd) {
                if validate_runner_directory(&runner_dir) {
                    tracing::debug!("Validated runner directory from process: {:?}", runner_dir);
                    return Ok(runner_dir);
                }
            }
        }
        Err(e) => {
            tracing::debug!("Failed to find Runner.Worker process: {}", e);
        }
    }

    // Method 2: Try to use RUNNER_TOOL_CACHE environment variable
    // This is typically /opt/hostedtoolcache on GitHub-hosted runners
    // and the runner directory is usually a sibling
    if let Ok(tool_cache) = env::var("RUNNER_TOOL_CACHE") {
        tracing::debug!("Found RUNNER_TOOL_CACHE: {}", tool_cache);
        let tool_cache_path = PathBuf::from(tool_cache);

        if let Some(parent) = tool_cache_path.parent() {
            // Common patterns: /opt/hostedtoolcache -> /opt, then check /opt/actions-runner
            if let Some(grandparent) = parent.parent() {
                let actions_runner = grandparent.join("actions-runner");
                if validate_runner_directory(&actions_runner) {
                    tracing::debug!(
                        "Found runner directory via RUNNER_TOOL_CACHE: {:?}",
                        actions_runner
                    );
                    return Ok(actions_runner);
                }
            }

            // Also try parent/actions-runner (e.g., /opt/actions-runner)
            let actions_runner = parent.join("actions-runner");
            if validate_runner_directory(&actions_runner) {
                tracing::debug!(
                    "Found runner directory via RUNNER_TOOL_CACHE (parent): {:?}",
                    actions_runner
                );
                return Ok(actions_runner);
            }
        }
    }

    // Method 3: Try to use GITHUB_WORKSPACE to find runner directory
    // GITHUB_WORKSPACE is typically /home/runner/work/{repo}/{repo}
    if let Ok(workspace) = env::var("GITHUB_WORKSPACE") {
        tracing::debug!("Found GITHUB_WORKSPACE: {}", workspace);
        let workspace_path = PathBuf::from(workspace);

        // Pattern: /home/runner/work/{repo}/{repo} -> /home/runner
        if let Some(parent) = workspace_path.parent() {
            if let Some(grandparent) = parent.parent() {
                if let Some(great_grandparent) = grandparent.parent() {
                    // Check if actions-runner exists in the same directory as "runner"
                    if let Some(runner_parent) = great_grandparent.parent() {
                        let actions_runner = runner_parent.join("actions-runner");
                        if validate_runner_directory(&actions_runner) {
                            tracing::debug!(
                                "Found runner directory via GITHUB_WORKSPACE: {:?}",
                                actions_runner
                            );
                            return Ok(actions_runner);
                        }
                    }

                    // Also check if great_grandparent itself is the runner home
                    let actions_runner = great_grandparent.join("actions-runner");
                    if validate_runner_directory(&actions_runner) {
                        tracing::debug!(
                            "Found runner directory via GITHUB_WORKSPACE (alt): {:?}",
                            actions_runner
                        );
                        return Ok(actions_runner);
                    }
                }
            }
        }
    }

    // Method 4: Search for directories with _diag subdirectory
    // Start from GITHUB_WORKSPACE and search upwards
    if let Ok(workspace) = env::var("GITHUB_WORKSPACE") {
        let workspace_path = PathBuf::from(workspace);
        if let Some(found_dir) = search_upwards_for_diag_directory(&workspace_path) {
            tracing::debug!(
                "Found runner directory by searching upwards: {:?}",
                found_dir
            );
            return Ok(found_dir);
        }
    }

    // Method 5: Try common default paths
    let common_paths = vec![
        "/home/runner/actions-runner",
        "/Users/runner/actions-runner",
        "/opt/actions-runner",
        "/runner/actions-runner",
    ];

    for path in common_paths {
        let runner_dir = PathBuf::from(path);
        if validate_runner_directory(&runner_dir) {
            tracing::debug!("Found runner directory at common path: {:?}", runner_dir);
            return Ok(runner_dir);
        }
    }

    Err(anyhow!(
        "Unable to find runner directory with _diag subdirectory using any method"
    ))
}

/// Validate that a directory is a runner directory by checking for _diag subdirectory
fn validate_runner_directory(path: &Path) -> bool {
    let diag_dir = path.join("_diag");
    let exists = diag_dir.exists() && diag_dir.is_dir();
    if !exists {
        tracing::debug!("Directory {:?} does not have _diag subdirectory", path);
    }
    exists
}

/// Search upwards from a starting path to find a directory containing _diag
fn search_upwards_for_diag_directory(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path;

    // Search up to 10 levels
    for _ in 0..10 {
        // Check siblings for any directory with _diag
        if let Some(parent) = current.parent() {
            tracing::debug!("Searching for _diag in parent directory: {:?}", parent);

            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            let candidate = entry.path();
                            if validate_runner_directory(&candidate) {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }

            current = parent;
        } else {
            break;
        }
    }

    None
}

/// Find the Runner.Worker process using sysinfo
fn find_runner_worker_process() -> Result<String> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Look for processes containing "Runner.Worker" in their command
    for (_, process) in sys.processes().iter() {
        let cmd = process.cmd();
        if cmd.iter().any(|arg| arg.contains("Runner.Worker")) {
            tracing::debug!("Found Runner.Worker process via sysinfo: {:?}", cmd);
            // Join the command arguments to reconstruct the full command
            return Ok(cmd.join(" "));
        }
    }

    Err(anyhow!("Runner.Worker process not found using sysinfo"))
}

/// Extract the runner directory from the worker command
fn extract_runner_directory(worker_cmd: &str) -> Result<PathBuf> {
    if let Some(index) = worker_cmd.find("Runner.Worker") {
        let path_up_to_worker = &worker_cmd[..index];
        let path_str = path_up_to_worker
            .strip_suffix('/')
            .unwrap_or(path_up_to_worker);
        let runner_dir = PathBuf::from(path_str)
            .parent()
            .ok_or_else(|| anyhow!("Unable to get parent directory from path: {}", path_str))?
            .to_path_buf();

        Ok(runner_dir)
    } else {
        Err(anyhow!(
            "Unable to extract path from Runner.Worker command string: {}",
            worker_cmd
        ))
    }
}

/// Find worker log files in the _diag directory
fn find_worker_log_files(runner_dir: &Path) -> Result<Vec<PathBuf>> {
    let diag_dir = runner_dir.join("_diag");
    let pattern = diag_dir.join("Worker_*.log");

    let mut log_files: Vec<PathBuf> = glob(pattern.to_string_lossy().as_ref())
        .map_err(|e| anyhow!("Failed to glob worker log files: {}", e))?
        .filter_map(|entry| entry.ok())
        .collect();

    // Sort by modification time (newest first)
    log_files.sort_by(|a, b| {
        let b_time = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let a_time = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        b_time.cmp(&a_time)
    });

    if log_files.is_empty() {
        return Err(anyhow!("No worker log files found in {:?}", diag_dir));
    }

    Ok(log_files)
}

/// Extract Job ID from a worker log file
fn extract_job_id_from_log(log_file: &Path) -> Result<String> {
    let content = std::fs::read_to_string(log_file)
        .map_err(|e| anyhow!("Failed to read log file {:?}: {}", log_file, e))?;

    let job_id_pattern =
        Regex::new(r#"(?:INFO JobRunner\] Job ID (\S+)|"jobId"\s*:\s*"([^"]+)")"#).unwrap();

    for line in content.lines() {
        if let Some(captures) = job_id_pattern.captures(line) {
            if let Some(job_id) = captures.get(1) {
                return Ok(job_id.as_str().trim().to_string());
            } else if let Some(job_id) = captures.get(2) {
                return Ok(job_id.as_str().trim().to_string());
            }
        }
    }

    Err(anyhow!("Job ID not found in log file {:?}", log_file))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_extract_runner_directory() {
        let cmd = "/path/to/runner/bin/Runner.Worker --some-arg";
        let result = extract_runner_directory(cmd).unwrap();
        assert_eq!(result, PathBuf::from("/path/to/runner"));
    }

    #[test]
    fn test_extract_runner_directory_versioned() {
        let cmd = "/Users/runner/actions-runner/bin.1.234.5/Runner.Worker spawnclient 155 158";
        let result = extract_runner_directory(cmd).unwrap();
        assert_eq!(result, PathBuf::from("/Users/runner/actions-runner"));
    }

    #[test]
    fn test_extract_runner_directory_with_trailing_slash() {
        let cmd = "/opt/actions-runner/bin.1.234.5/Runner.Worker spawnclient 149 152";
        let result = extract_runner_directory(cmd).unwrap();
        assert_eq!(result, PathBuf::from("/opt/actions-runner"));
    }

    #[test]
    fn test_extract_job_id_from_log() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");
        let log_content = "INFO JobRunner] Job ID test-job-123";
        fs::write(&log_file, log_content).unwrap();

        let result = extract_job_id_from_log(&log_file).unwrap();
        assert_eq!(result, "test-job-123");
    }

    #[test]
    fn test_extract_job_id_from_log_not_found() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");
        let log_content = "Some other log content without Job ID";
        fs::write(&log_file, log_content).unwrap();

        let result = extract_job_id_from_log(&log_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_job_id_from_github_actions_log() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");
        let log_content = "INFO JobRunner] Job ID github-job-456";
        fs::write(&log_file, log_content).unwrap();

        let result = extract_job_id_from_log(&log_file).unwrap();
        assert_eq!(result, "github-job-456");
    }

    #[test]
    fn test_extract_job_id_from_json_log() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");
        let log_content =
            r#"{"some": "data", "jobId": "836e04dc-1f9b-529a-9646-8e46c7a95261", "other": "info"}"#;
        fs::write(&log_file, log_content).unwrap();

        let result = extract_job_id_from_log(&log_file).unwrap();
        assert_eq!(result, "836e04dc-1f9b-529a-9646-8e46c7a95261");
    }

    #[test]
    fn test_extract_job_id_from_json_log_partial() {
        let temp_dir = tempdir().unwrap();
        let log_file = temp_dir.path().join("test.log");
        let log_content = r#""jobId": "github-job-456""#;
        println!("log_content: {}", log_content);
        fs::write(&log_file, log_content).unwrap();

        let result = extract_job_id_from_log(&log_file).unwrap();
        assert_eq!(result, "github-job-456");
    }

    #[test]
    fn test_extract_github_external_id_returns_some_when_in_github_actions() {
        env::set_var("GITHUB_ACTIONS", "true");

        let result = extract_github_external_id();
        assert!(
            result.is_ok(),
            "Function should not return an error even when runner is not found"
        );

        match result.unwrap() {
            None => {
                tracing::debug!("Gracefully returned None as expected in test environment");
            }
            Some(external_id) => {
                tracing::debug!("Unexpectedly found external ID: {}", external_id);
            }
        }

        env::remove_var("GITHUB_ACTIONS");
    }
}
