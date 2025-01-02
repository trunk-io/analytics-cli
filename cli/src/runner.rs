use std::time::SystemTime;

use bundle::{FileSet, FileSetCounter};
use codeowners::CodeOwners;
use context::junit::junit_path::JunitReportFileWithStatus;

pub fn build_filesets(
    repo_root: &str,
    junit_paths: &[JunitReportFileWithStatus],
    team: Option<String>,
    codeowners: &Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<(Vec<FileSet>, FileSetCounter)> {
    let mut file_counter = FileSetCounter::default();
    let mut file_sets = junit_paths
        .iter()
        .map(|junit_wrapper| {
            FileSet::scan_from_glob(
                repo_root,
                junit_wrapper.clone(),
                &mut file_counter,
                team.clone(),
                codeowners,
                exec_start,
            )
        })
        .collect::<anyhow::Result<Vec<FileSet>>>()?;

    // Handle case when junit paths are not globs.
    if file_counter.get_count() == 0 {
        file_sets = junit_paths
            .iter()
            .map(|junit_wrapper| {
                let mut path = junit_wrapper.junit_path.clone();
                if !path.ends_with('/') {
                    path.push('/');
                }
                path.push_str("**/*.xml");
                FileSet::scan_from_glob(
                    repo_root,
                    JunitReportFileWithStatus {
                        junit_path: path,
                        status: junit_wrapper.status.clone(),
                    },
                    &mut file_counter,
                    team.clone(),
                    codeowners,
                    exec_start,
                )
            })
            .collect::<anyhow::Result<Vec<FileSet>>>()?;
    }

    Ok((file_sets, file_counter))
}
