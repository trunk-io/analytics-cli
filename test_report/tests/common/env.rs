use std::env;

use constants::{
    TRUNK_ALLOW_EMPTY_TEST_RESULTS_ENV, TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_API_TOKEN_ENV,
    TRUNK_CODEOWNERS_PATH_ENV, TRUNK_DISABLE_QUARANTINING_ENV, TRUNK_DRY_RUN_ENV,
    TRUNK_ORG_URL_SLUG_ENV, TRUNK_PR_NUMBER_ENV, TRUNK_PUBLIC_API_ADDRESS_ENV,
    TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, TRUNK_REPO_HEAD_AUTHOR_NAME_ENV,
    TRUNK_REPO_HEAD_BRANCH_ENV, TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV, TRUNK_REPO_HEAD_SHA_ENV,
    TRUNK_REPO_ROOT_ENV, TRUNK_REPO_URL_ENV, TRUNK_USE_UNCLONED_REPO_ENV, TRUNK_VARIANT_ENV,
};

/// Cleans up all TRUNK_* and CI-related environment variables to avoid test interference.
pub fn cleanup_env_vars() {
    unsafe {
        env::remove_var(TRUNK_PUBLIC_API_ADDRESS_ENV);
        env::remove_var(TRUNK_API_TOKEN_ENV);
        env::remove_var(TRUNK_ORG_URL_SLUG_ENV);
        env::remove_var(TRUNK_PR_NUMBER_ENV);
        env::remove_var(TRUNK_REPO_ROOT_ENV);
        env::remove_var(TRUNK_REPO_URL_ENV);
        env::remove_var(TRUNK_REPO_HEAD_SHA_ENV);
        env::remove_var(TRUNK_REPO_HEAD_BRANCH_ENV);
        env::remove_var(TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV);
        env::remove_var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV);
        env::remove_var(TRUNK_VARIANT_ENV);
        env::remove_var(TRUNK_USE_UNCLONED_REPO_ENV);
        env::remove_var(TRUNK_DISABLE_QUARANTINING_ENV);
        env::remove_var(TRUNK_ALLOW_EMPTY_TEST_RESULTS_ENV);
        env::remove_var(TRUNK_DRY_RUN_ENV);
        env::remove_var(TRUNK_CODEOWNERS_PATH_ENV);
        env::remove_var("CI");
        env::remove_var("GITHUB_JOB");
        env::remove_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV);
        env::remove_var(TRUNK_API_CLIENT_RETRY_COUNT_ENV);
    }
}

#[allow(dead_code)]
pub fn clean_up_cache_files() {
    let cache_dir = env::temp_dir().join(constants::CACHE_DIR);
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
