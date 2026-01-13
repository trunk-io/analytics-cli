use context::env::{
    self, EnvVars,
    parser::{BranchClass, CIInfo, CIPlatform, EnvParser},
    validator::{EnvValidationIssue, EnvValidationIssueSubOptimal, EnvValidationLevel},
};

#[path = "env/bitbucket.rs"]
mod bitbucket;
#[path = "env/buildkite.rs"]
mod buildkite;
#[path = "env/circleci.rs"]
mod circleci;
#[path = "env/custom.rs"]
mod custom;
#[path = "env/drone.rs"]
mod drone;
#[path = "env/github.rs"]
mod github;
#[path = "env/gitlab.rs"]
mod gitlab;
#[path = "env/semaphore.rs"]
mod semaphore;
