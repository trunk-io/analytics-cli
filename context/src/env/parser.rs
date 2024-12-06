#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::string_safety::safe_truncate_string;

use super::EnvVars;

// TODO(TRUNK-12908): Switch to using a crate for parsing the CI platform and related env vars
mod ci_platform_env_key {
    /// https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/store-information-in-variables#default-environment-variables
    pub const GITHUB_ACTIONS: &str = "GITHUB_ACTIONS";
    /// https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#using-environment-variables
    pub const JENKINS_PIPELINE: &str = "BUILD_ID";
    /// https://circleci.com/docs/variables/#built-in-environment-variables
    pub const CIRCLECI: &str = "CIRCLECI";
    /// https://buildkite.com/docs/pipelines/environment-variables#buildkite-environment-variables
    pub const BUILDKITE: &str = "BUILDKITE";
    /// https://docs.semaphoreci.com/ci-cd-environment/environment-variables/#semaphore
    pub const SEMAPHORE: &str = "SEMAPHORE";
    /// https://docs.travis-ci.com/user/environment-variables/#default-environment-variables
    pub const TRAVIS_CI: &str = "TRAVIS";
    /// https://docs.webapp.io/layerfile-reference/build-env#webappio
    pub const WEBAPPIO: &str = "WEBAPPIO";
    /// https://docs.aws.amazon.com/codebuild/latest/userguide/build-env-ref-env-vars.html
    pub const AWS_CODEBUILD: &str = "CODEBUILD_BUILD_ID";
    /// https://support.atlassian.com/bitbucket-cloud/docs/variables-and-secrets/
    pub const BITBUCKET: &str = "BITBUCKET_BUILD_NUMBER";
    /// https://learn.microsoft.com/en-us/azure/devops/pipelines/build/variables?view=azure-devops&tabs=yaml#system-variables-devops-services
    pub const AZURE_PIPELINES: &str = "TF_BUILD";
    /// https://docs.gitlab.com/ee/ci/variables/predefined_variables.html#predefined-variables
    pub const GITLAB_CI: &str = "GITLAB_CI";
    /// https://docs.drone.io/pipeline/environment/reference/drone/
    pub const DRONE: &str = "DRONE";
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CIPlatform {
    GitHubActions,
    JenkinsPipeline,
    CircleCI,
    Buildkite,
    Semaphore,
    TravisCI,
    Webappio,
    AWSCodeBuild,
    BitbucketPipelines,
    AzurePipelines,
    GitLabCI,
    Drone,
    Unknown,
}

impl From<&str> for CIPlatform {
    fn from(value: &str) -> Self {
        match value {
            ci_platform_env_key::GITHUB_ACTIONS => CIPlatform::GitHubActions,
            ci_platform_env_key::JENKINS_PIPELINE => CIPlatform::JenkinsPipeline,
            ci_platform_env_key::CIRCLECI => CIPlatform::CircleCI,
            ci_platform_env_key::BUILDKITE => CIPlatform::Buildkite,
            ci_platform_env_key::SEMAPHORE => CIPlatform::Semaphore,
            ci_platform_env_key::TRAVIS_CI => CIPlatform::TravisCI,
            ci_platform_env_key::WEBAPPIO => CIPlatform::Webappio,
            ci_platform_env_key::AWS_CODEBUILD => CIPlatform::AWSCodeBuild,
            ci_platform_env_key::BITBUCKET => CIPlatform::BitbucketPipelines,
            ci_platform_env_key::AZURE_PIPELINES => CIPlatform::AzurePipelines,
            ci_platform_env_key::GITLAB_CI => CIPlatform::GitLabCI,
            ci_platform_env_key::DRONE => CIPlatform::Drone,
            _ => CIPlatform::Unknown,
        }
    }
}

impl From<&EnvVars> for CIPlatform {
    fn from(value: &EnvVars) -> Self {
        let mut ci_platform = CIPlatform::Unknown;
        for (key, ..) in value.iter() {
            ci_platform = CIPlatform::from(key.as_str());
            if ci_platform != CIPlatform::Unknown {
                break;
            }
        }
        ci_platform
    }
}

const MAX_BRANCH_NAME_SIZE: usize = 1000;

#[derive(Debug, Clone)]
pub struct CIInfoParser<'a> {
    ci_info: CIInfo,
    env_vars: &'a EnvVars,
}

impl<'a> CIInfoParser<'a> {
    pub fn new(platform: CIPlatform, env_vars: &'a EnvVars) -> Self {
        Self {
            ci_info: CIInfo::new(platform),
            env_vars,
        }
    }

    pub fn ci_info(&self) -> &CIInfo {
        &self.ci_info
    }

    pub fn info_ci_info(self) -> CIInfo {
        self.ci_info
    }

    pub fn parse(&mut self) {
        match self.ci_info.platform {
            CIPlatform::GitHubActions => self.parse_github_actions(),
            CIPlatform::JenkinsPipeline => self.parse_jenkins_pipeline(),
            CIPlatform::Buildkite => self.parse_buildkite(),
            CIPlatform::Semaphore => self.parse_semaphore(),
            CIPlatform::GitLabCI => self.parse_gitlab_ci(),
            CIPlatform::Drone => self.parse_drone(),
            CIPlatform::CircleCI
            | CIPlatform::TravisCI
            | CIPlatform::Webappio
            | CIPlatform::AWSCodeBuild
            | CIPlatform::BitbucketPipelines
            | CIPlatform::AzurePipelines
            | CIPlatform::Unknown => {
                // TODO(TRUNK-12908): Switch to using a crate for parsing the CI platform and related env vars
                // TODO(TRUNK-12909): parse more platforms
            }
        };
        self.clean_branch();
        self.parse_branch_class();
    }

    fn clean_branch(&mut self) {
        if let Some(branch) = &mut self.ci_info.branch {
            *branch = clean_branch(branch);
        }
    }

    fn parse_branch_class(&mut self) {
        if let Some(branch) = &self.ci_info.branch {
            self.ci_info.branch_class =
                Some(BranchClass::from((branch.as_str(), self.ci_info.pr_number)));
        }
    }

    fn parse_github_actions(&mut self) {
        if let Some(gh_ref) = self.get_env_var("GITHUB_REF") {
            if gh_ref.starts_with("refs/pull/") {
                let stripped_ref = gh_ref
                    .strip_suffix("/merge")
                    .unwrap_or(gh_ref.as_str())
                    .splitn(3, "/")
                    .last();
                self.ci_info.pr_number = Self::parse_pr_number(stripped_ref);
            }
            if let Some(gh_head_ref) = self.get_env_var("GITHUB_HEAD_REF") {
                self.ci_info.branch = Some(gh_head_ref);
            } else {
                self.ci_info.branch = Some(gh_ref);
            }
        }

        self.ci_info.actor = self.get_env_var("GITHUB_ACTOR");
        if let (Some(repo_name), Some(run_id)) = (
            self.get_env_var("GITHUB_REPOSITORY"),
            self.get_env_var("GITHUB_RUN_ID"),
        ) {
            let mut job_url = format!("https://github.com/{repo_name}/actions/runs/{run_id}");
            if let Some(pr_number) = self.ci_info.pr_number {
                job_url = format!("{job_url}?pr={pr_number}");
            }
            self.ci_info.job_url = Some(job_url);
        }
        self.ci_info.workflow = self.get_env_var("GITHUB_WORKFLOW");
        self.ci_info.job = self.get_env_var("GITHUB_JOB");
    }

    fn parse_jenkins_pipeline(&mut self) {
        self.ci_info.job_url = self.get_env_var("BUILD_URL");
        self.ci_info.branch = self
            .get_env_var("CHANGE_BRANCH")
            .or_else(|| self.get_env_var("BRANCH_NAME"));
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("CHANGE_ID"));
        self.ci_info.actor = self.get_env_var("CHANGE_AUTHOR_EMAIL");
        self.ci_info.committer_name = self.get_env_var("CHANGE_AUTHOR_DISPLAY_NAME");
        self.ci_info.committer_email = self.get_env_var("CHANGE_AUTHOR_EMAIL");
        self.ci_info.author_name = self.get_env_var("CHANGE_AUTHOR_DISPLAY_NAME");
        self.ci_info.author_email = self.get_env_var("CHANGE_AUTHOR_EMAIL");
    }

    fn parse_buildkite(&mut self) {
        self.ci_info.job_url = self.get_env_var("BUILDKITE_BUILD_URL");
        self.ci_info.branch = self.get_env_var("BUILDKITE_BRANCH");
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("BUILDKITE_PULL_REQUEST"));
        self.ci_info.actor = self.get_env_var("BUILDKITE_BUILD_AUTHOR_EMAIL");
        self.ci_info.committer_name = self.get_env_var("BUILDKITE_BUILD_AUTHOR");
        self.ci_info.committer_email = self.get_env_var("BUILDKITE_BUILD_AUTHOR_EMAIL");
        self.ci_info.author_name = self.get_env_var("BUILDKITE_BUILD_AUTHOR");
        self.ci_info.author_email = self.get_env_var("BUILDKITE_BUILD_AUTHOR_EMAIL");
    }

    fn parse_semaphore(&mut self) {
        if let (Some(org_url), Some(project_id), Some(job_id)) = (
            self.get_env_var("SEMAPHORE_ORGANIZATION_URL"),
            self.get_env_var("SEMAPHORE_PROJECT_ID"),
            self.get_env_var("SEMAPHORE_JOB_ID"),
        ) {
            self.ci_info.job_url = Some(format!("{org_url}/projects/{project_id}/jobs/{job_id}"));
        }
        self.ci_info.branch = self
            .get_env_var("SEMAPHORE_GIT_PR_BRANCH")
            .or_else(|| self.get_env_var("SEMAPHORE_GIT_WORKING_BRANCH"))
            .or_else(|| self.get_env_var("SEMAPHORE_GIT_BRANCH"));
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("SEMAPHORE_GIT_PR_NUMBER"));
        self.ci_info.actor = self.get_env_var("SEMAPHORE_GIT_COMMIT_AUTHOR");
        self.ci_info.committer_name = self.get_env_var("SEMAPHORE_GIT_COMMITTER");
        self.ci_info.author_name = self.get_env_var("SEMAPHORE_GIT_COMMIT_AUTHOR");

        self.ci_info.workflow = self.get_env_var("SEMAPHORE_PROJECT_NAME");
        self.ci_info.job = self.get_env_var("SEMAPHORE_JOB_NAME");
    }

    fn parse_gitlab_ci(&mut self) {
        self.ci_info.job_url = self.get_env_var("CI_JOB_URL");
        if let Some(branch) = self
            .get_env_var("CI_COMMIT_REF_NAME")
            .or_else(|| self.get_env_var("CI_COMMIT_BRANCH"))
            .or_else(|| self.get_env_var("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME"))
        {
            self.ci_info.branch = Some(if branch.starts_with("remotes/") {
                branch.replacen("remotes/", "", 1)
            } else {
                branch
            });
        }
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("CI_MERGE_REQUEST_IID"));
        // `CI_COMMIT_AUTHOR` has format `Name <email>`
        // https://docs.gitlab.com/ee/ci/variables/predefined_variables.html
        if let Some((name, email)) = self
            .get_env_var("CI_COMMIT_AUTHOR")
            .as_ref()
            .and_then(|author| author.split_once('<'))
            .map(|(name_with_space, email_with_end_angle_bracket)| {
                (
                    String::from(name_with_space.trim()),
                    email_with_end_angle_bracket.replace('>', ""),
                )
            })
        {
            self.ci_info.actor = Some(name.clone());
            self.ci_info.committer_name = Some(name.clone());
            self.ci_info.committer_email = Some(email.clone());
            self.ci_info.author_name = Some(name);
            self.ci_info.author_email = Some(email);
        }
        self.ci_info.commit_message = self.get_env_var("CI_COMMIT_MESSAGE");
        self.ci_info.title = self.get_env_var("CI_MERGE_REQUEST_TITLE");
        self.ci_info.workflow = self.get_env_var("CI_JOB_NAME");
        self.ci_info.job = self.get_env_var("CI_JOB_STAGE");
    }

    fn parse_drone(&mut self) {
        self.ci_info.branch = self.get_env_var("DRONE_SOURCE_BRANCH");
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("DRONE_PULL_REQUEST"));
        self.ci_info.actor = self.get_env_var("DRONE_COMMIT_AUTHOR");
        self.ci_info.committer_name = self.get_env_var("DRONE_COMMIT_AUTHOR_NAME");
        self.ci_info.committer_email = self.get_env_var("DRONE_COMMIT_AUTHOR_EMAIL");
        self.ci_info.author_name = self.get_env_var("DRONE_COMMIT_AUTHOR_NAME");
        self.ci_info.author_email = self.get_env_var("DRONE_COMMIT_AUTHOR_EMAIL");
        self.ci_info.title = self.get_env_var("DRONE_PULL_REQUEST_TITLE");
        self.ci_info.job_url = self.get_env_var("DRONE_BUILD_LINK");
    }

    fn get_env_var<T: AsRef<str>>(&self, env_var: T) -> Option<String> {
        self.env_vars
            .get(env_var.as_ref())
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .cloned()
    }

    fn parse_pr_number<T: AsRef<str>>(env_var: Option<T>) -> Option<usize> {
        env_var.and_then(|pr_number_str| pr_number_str.as_ref().parse::<usize>().ok())
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CIInfo {
    pub platform: CIPlatform,
    pub job_url: Option<String>,
    pub branch: Option<String>,
    pub branch_class: Option<BranchClass>,
    pub pr_number: Option<usize>,
    pub actor: Option<String>,
    pub committer_name: Option<String>,
    pub committer_email: Option<String>,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub commit_message: Option<String>,
    pub title: Option<String>,
    pub workflow: Option<String>,
    pub job: Option<String>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchClass {
    PullRequest,
    ProtectedBranch,
    Merge,
    None,
}

impl From<(&str, Option<usize>)> for BranchClass {
    fn from(value: (&str, Option<usize>)) -> Self {
        let (branch_name, pr_number) = value;
        if pr_number.is_some() {
            return BranchClass::PullRequest;
        }
        if branch_name.starts_with("remotes/pull/") || branch_name.starts_with("pull/") {
            BranchClass::PullRequest
        } else if matches!(branch_name, "master" | "main") {
            BranchClass::ProtectedBranch
        } else if branch_name.contains("/trunk-merge/") {
            BranchClass::Merge
        } else {
            BranchClass::None
        }
    }
}

pub fn clean_branch(branch: &str) -> String {
    let new_branch = branch
        .replace("refs/heads/", "")
        .replace("refs/", "")
        .replace("origin/", "");

    return String::from(safe_truncate_string::<MAX_BRANCH_NAME_SIZE, _>(&new_branch));
}

impl CIInfo {
    pub fn new(platform: CIPlatform) -> Self {
        Self {
            platform,
            job_url: None,
            branch: None,
            branch_class: None,
            pr_number: None,
            actor: None,
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: None,
            job: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EnvParser<'a> {
    ci_info_parser: Option<CIInfoParser<'a>>,
}

impl<'a> EnvParser<'a> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn ci_info_parser(&self) -> &Option<CIInfoParser> {
        &self.ci_info_parser
    }

    pub fn into_ci_info_parser(self) -> Option<CIInfoParser<'a>> {
        self.ci_info_parser
    }

    pub fn parse(&mut self, env_vars: &'a EnvVars) {
        self.parse_ci_platform(env_vars);
        if let Some(ci_info) = &mut self.ci_info_parser {
            ci_info.parse();
        }
    }

    fn parse_ci_platform(&mut self, env_vars: &'a EnvVars) {
        self.ci_info_parser = Some(CIInfoParser::new(CIPlatform::from(env_vars), &env_vars));
    }
}
