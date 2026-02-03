#[cfg(feature = "ruby")]
use magnus::{Module, Object, value::ReprValue};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use super::EnvVars;
use crate::repo::BundleRepo;
use crate::string_safety::safe_truncate_string;

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
    /// https://confluence.atlassian.com/bamboo/bamboo-variables-289277087.html
    pub const BAMBOO: &str = "bamboo_buildNumber";
    /// Custom environment allowing users to manually set upload metadata using these env vars, all of which are optional:
    /// JOB_URL: Url for the ci job that was run
    /// JOB_NAME: Name of the ci job that was run
    /// AUTHOR_EMAIL: Email for the pr author
    /// AUTHOR_NAME: Display name for the pr author
    /// COMMIT_BRANCH: Branch for the pr run
    /// COMMIT_MESSAGE: Message for the commit run
    /// PR_NUMBER: Number for the pr (must actually be a number)
    /// PR_TITLE: Title of the pr
    pub const CUSTOM: &str = "CUSTOM";
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "CIPlatform"))]
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
    Bamboo,
    Custom,
    Unknown,
}

impl From<CIPlatform> for &str {
    fn from(val: CIPlatform) -> Self {
        match val {
            CIPlatform::GitHubActions => ci_platform_env_key::GITHUB_ACTIONS,
            CIPlatform::JenkinsPipeline => ci_platform_env_key::JENKINS_PIPELINE,
            CIPlatform::CircleCI => ci_platform_env_key::CIRCLECI,
            CIPlatform::Buildkite => ci_platform_env_key::BUILDKITE,
            CIPlatform::Semaphore => ci_platform_env_key::SEMAPHORE,
            CIPlatform::TravisCI => ci_platform_env_key::TRAVIS_CI,
            CIPlatform::Webappio => ci_platform_env_key::WEBAPPIO,
            CIPlatform::AWSCodeBuild => ci_platform_env_key::AWS_CODEBUILD,
            CIPlatform::BitbucketPipelines => ci_platform_env_key::BITBUCKET,
            CIPlatform::AzurePipelines => ci_platform_env_key::AZURE_PIPELINES,
            CIPlatform::GitLabCI => ci_platform_env_key::GITLAB_CI,
            CIPlatform::Drone => ci_platform_env_key::DRONE,
            CIPlatform::Bamboo => ci_platform_env_key::BAMBOO,
            CIPlatform::Custom => ci_platform_env_key::CUSTOM,
            CIPlatform::Unknown => "UNKNOWN",
        }
    }
}

impl ToString for CIPlatform {
    fn to_string(&self) -> String {
        String::from(Into::<&str>::into(*self))
    }
}

impl CIPlatform {
    #[cfg(feature = "ruby")]
    pub fn to_string(&self) -> &str {
        (*self).into()
    }

    // Enforcing a priority rule for CIPlatforms. Highest priority is Custom,
    // lowest is Unknown, everything else is in the middle (ie we choose arbitrarily
    // between the two)
    fn prioritize(self, other: CIPlatform) -> CIPlatform {
        match (self, other) {
            (CIPlatform::Custom, _) => CIPlatform::Custom,
            (_, CIPlatform::Custom) => CIPlatform::Custom,
            (CIPlatform::Unknown, anything_else) => anything_else,
            (anything_else, CIPlatform::Unknown) => anything_else,
            (_, _) => self,
        }
    }
}

#[cfg(feature = "ruby")]
impl magnus::TryConvert for CIPlatform {
    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {
        let ival: i32 = val.funcall("to_i", ())?;
        match ival {
            0 => Ok(CIPlatform::GitHubActions),
            1 => Ok(CIPlatform::JenkinsPipeline),
            2 => Ok(CIPlatform::CircleCI),
            3 => Ok(CIPlatform::Buildkite),
            4 => Ok(CIPlatform::Semaphore),
            5 => Ok(CIPlatform::TravisCI),
            6 => Ok(CIPlatform::Webappio),
            7 => Ok(CIPlatform::AWSCodeBuild),
            8 => Ok(CIPlatform::BitbucketPipelines),
            9 => Ok(CIPlatform::AzurePipelines),
            10 => Ok(CIPlatform::GitLabCI),
            11 => Ok(CIPlatform::Drone),
            12 => Ok(CIPlatform::Custom),
            13 => Ok(CIPlatform::Bamboo),
            _ => Err(magnus::Error::new(
                magnus::Ruby::get_with(val).exception_type_error(),
                format!("invalid CIPlatform: {}", val),
            )),
        }
    }
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
            ci_platform_env_key::BAMBOO => CIPlatform::Bamboo,
            ci_platform_env_key::CUSTOM => CIPlatform::Custom,
            _ => CIPlatform::Unknown,
        }
    }
}

impl From<&EnvVars> for CIPlatform {
    fn from(value: &EnvVars) -> Self {
        let mut ci_platform = CIPlatform::Unknown;
        for (key, ..) in value.iter() {
            ci_platform = ci_platform.prioritize(CIPlatform::from(key.as_str()));
        }
        ci_platform
    }
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum CIInfoParseError {
    #[error("could not parse GitLab merge request event type")]
    GitLabMergeRequestEventType,
}

const MAX_BRANCH_NAME_SIZE: usize = 1000;

#[derive(Debug, Clone)]
pub struct CIInfoParser<'a> {
    errors: Vec<CIInfoParseError>,
    ci_info: CIInfo,
    env_vars: &'a EnvVars,
    stable_branches: &'a [&'a str],
    repo: Option<&'a BundleRepo>,
}

impl<'a> CIInfoParser<'a> {
    pub fn new(
        platform: CIPlatform,
        env_vars: &'a EnvVars,
        stable_branches: &'a [&'a str],
        repo: Option<&'a BundleRepo>,
    ) -> Self {
        Self {
            errors: Vec::new(),
            ci_info: CIInfo::new(platform),
            env_vars,
            stable_branches,
            repo,
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
            CIPlatform::BitbucketPipelines => self.parse_bitbucket_pipelines(),
            CIPlatform::CircleCI => self.parse_circleci(),
            CIPlatform::Bamboo => self.parse_bamboo(),
            CIPlatform::Custom => self.parse_custom_info(),
            CIPlatform::TravisCI
            | CIPlatform::Webappio
            | CIPlatform::AWSCodeBuild
            | CIPlatform::AzurePipelines
            | CIPlatform::Unknown => {
                // TODO(TRUNK-12908): Switch to using a crate for parsing the CI platform and related env vars
                // TODO(TRUNK-12909): parse more platforms
            }
        };
        self.clean_branch();
        self.parse_branch_class();
        self.apply_repo_overrides();
    }

    fn clean_branch(&mut self) {
        if let Some(branch) = &mut self.ci_info.branch {
            *branch = clean_branch(branch);
        }
    }

    fn parse_branch_class(&mut self) {
        if let Some(branch) = &self.ci_info.branch {
            let mut merge_request_event_type: Option<GitLabMergeRequestEventType> = None;
            if let Some(env_event_type) = self.get_env_var("CI_MERGE_REQUEST_EVENT_TYPE") {
                match GitLabMergeRequestEventType::try_from(env_event_type.as_str()) {
                    Ok(event_type) => {
                        merge_request_event_type = Some(event_type);
                    }
                    Err(err) => {
                        self.errors.push(err);
                    }
                }
            }

            self.ci_info.branch_class = Some(BranchClass::from((
                branch.as_str(),
                self.ci_info.pr_number,
                merge_request_event_type,
                self.stable_branches,
            )));
        }
    }

    fn parse_custom_info(&mut self) {
        self.ci_info.job_url = self.get_env_var("JOB_URL");
        self.ci_info.workflow = self.get_env_var("JOB_NAME");
        self.ci_info.job = self.get_env_var("JOB_NAME");

        self.ci_info.actor = self.get_env_var("AUTHOR_EMAIL");
        self.ci_info.committer_email = self.get_env_var("AUTHOR_EMAIL");
        self.ci_info.author_email = self.get_env_var("AUTHOR_EMAIL");

        self.ci_info.committer_name = self.get_env_var("AUTHOR_NAME");
        self.ci_info.author_name = self.get_env_var("AUTHOR_NAME");

        self.ci_info.branch = self.get_env_var("COMMIT_BRANCH");
        self.ci_info.commit_message = self.get_env_var("COMMIT_MESSAGE");

        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("PR_NUMBER"));
        self.ci_info.title = self.get_env_var("PR_TITLE");
    }

    fn parse_github_actions(&mut self) {
        if let Some(gh_ref) = self.get_env_var("GITHUB_REF") {
            if gh_ref.starts_with("refs/pull/") {
                let stripped_ref = gh_ref
                    .strip_suffix("/merge")
                    .unwrap_or(gh_ref.as_str())
                    .splitn(3, '/')
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
        self.ci_info.title = self.get_env_var("PR_TITLE");
        let job_url = match (
            self.get_env_var("JOB_URL"),
            self.get_env_var("GITHUB_REPOSITORY"),
            self.get_env_var("GITHUB_RUN_ID"),
        ) {
            (Some(job_url), _, _) => Some(job_url),
            (None, Some(repo_name), Some(run_id)) => {
                let mut job_url = format!("https://github.com/{repo_name}/actions/runs/{run_id}");
                if let Some(pr_number) = self.ci_info.pr_number {
                    job_url = format!("{job_url}?pr={pr_number}");
                }
                Some(job_url)
            }
            _ => None,
        };
        self.ci_info.job_url = job_url;
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
        if let (Some(url), Some(id)) = (
            self.get_env_var("BUILDKITE_BUILD_URL"),
            self.get_env_var("BUILDKITE_JOB_ID"),
        ) {
            self.ci_info.job_url = Some(format!("{}#{}", url, id));
        }
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

    fn parse_bitbucket_pipelines(&mut self) {
        // Construct job URL from workspace, repo slug, and build number
        // Format: https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}
        // With step: https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}/steps/{step_uuid}

        if let (Some(workspace), Some(repo_slug), Some(build_number)) = (
            self.get_env_var("BITBUCKET_WORKSPACE"),
            self.get_env_var("BITBUCKET_REPO_SLUG"),
            self.get_env_var("BITBUCKET_BUILD_NUMBER"),
        ) {
            self.ci_info.job_url = Some({
                let base_url = format!(
                    "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}"
                );
                if let Some(step_uuid) = self.get_env_var("BITBUCKET_STEP_UUID") {
                    // URL-encode the step UUID for use in the URL path
                    let encoded_step_uuid = url_encode_path_segment(&step_uuid);
                    format!("{base_url}/steps/{encoded_step_uuid}")
                } else {
                    base_url
                }
            });
        }

        self.ci_info.branch = self.get_env_var("BITBUCKET_BRANCH");
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("BITBUCKET_PR_ID"));

        // Use pipeline UUID as workflow identifier and step UUID as job identifier
        self.ci_info.workflow = self.get_env_var("BITBUCKET_PIPELINE_UUID");
        self.ci_info.job = self.get_env_var("BITBUCKET_STEP_UUID");

        // Note: Bitbucket Pipelines doesn't provide author/committer info, commit message,
        // or PR title via environment variables. These will be populated from repo info
        // via apply_repo_overrides(), or users can set them via CUSTOM env vars.
    }

    fn parse_circleci(&mut self) {
        self.ci_info.job_url = self.get_env_var("CIRCLE_BUILD_URL");
        self.ci_info.branch = self.get_env_var("CIRCLE_BRANCH");
        self.ci_info.pr_number = Self::parse_pr_number(self.get_env_var("CIRCLE_PR_NUMBER"));
        self.ci_info.actor = self.get_env_var("CIRCLE_USERNAME");

        self.ci_info.workflow = self.get_env_var("CIRCLE_WORKFLOW_ID");
        self.ci_info.job = self.get_env_var("CIRCLE_JOB");
    }

    fn parse_bamboo(&mut self) {
        self.ci_info.job_url = self
            .get_env_var("bamboo_buildResultsUrl")
            .or_else(|| self.get_env_var("bamboo_resultsUrl"));
        self.ci_info.branch = self
            .get_env_var("bamboo_planRepository_branch")
            .or_else(|| self.get_env_var("bamboo_planRepository_branchName"));
        self.ci_info.pr_number =
            Self::parse_pr_number(self.get_env_var("bamboo_repository_pr_key"));
        self.ci_info.actor = self.get_env_var("bamboo_planRepository_username");
        self.ci_info.workflow = self.get_env_var("bamboo_planName");
        self.ci_info.job = self.get_env_var("bamboo_shortJobName");
    }

    fn get_env_var<T: AsRef<str>>(&self, env_var: T) -> Option<String> {
        self.env_vars
            .get(env_var.as_ref())
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .cloned()
    }

    fn apply_repo_overrides(&mut self) {
        let repo = match self.repo {
            Some(repo) => repo,
            None => return,
        };

        let prefer_repo_values = repo.use_uncloned_repo.unwrap_or(false);

        if prefer_repo_values || self.ci_info.branch.is_none() {
            let new_branch = clean_branch(&repo.repo_head_branch);
            self.ci_info.branch = Some(new_branch);
            // Recalculate branch_class after branch override
            if let Some(branch) = &self.ci_info.branch {
                let mut merge_request_event_type: Option<GitLabMergeRequestEventType> = None;
                if let Some(env_event_type) = self.get_env_var("CI_MERGE_REQUEST_EVENT_TYPE") {
                    if let Ok(event_type) =
                        GitLabMergeRequestEventType::try_from(env_event_type.as_str())
                    {
                        merge_request_event_type = Some(event_type);
                    }
                }
                self.ci_info.branch_class = Some(BranchClass::from((
                    branch.as_str(),
                    self.ci_info.pr_number,
                    merge_request_event_type,
                    self.stable_branches,
                )));
            }
        }
        if prefer_repo_values || self.ci_info.actor.is_none() {
            self.ci_info.actor = Some(repo.repo_head_author_email.clone());
        }
        if prefer_repo_values || self.ci_info.committer_name.is_none() {
            self.ci_info.committer_name = Some(repo.repo_head_author_name.clone());
        }
        if prefer_repo_values || self.ci_info.committer_email.is_none() {
            self.ci_info.committer_email = Some(repo.repo_head_author_email.clone());
        }
        if prefer_repo_values || self.ci_info.author_name.is_none() {
            self.ci_info.author_name = Some(repo.repo_head_author_name.clone());
        }
        if prefer_repo_values || self.ci_info.author_email.is_none() {
            self.ci_info.author_email = Some(repo.repo_head_author_email.clone());
        }
        if prefer_repo_values || self.ci_info.commit_message.is_none() {
            self.ci_info.commit_message = Some(repo.repo_head_commit_message.clone());
        }
    }

    fn parse_pr_number<T: AsRef<str>>(env_var: Option<T>) -> Option<usize> {
        env_var.and_then(|pr_number_str| pr_number_str.as_ref().parse::<usize>().ok())
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(
    feature = "ruby",
    magnus::wrap(class = "CIInfo", free_immediately, size)
)]
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
#[cfg_attr(feature = "ruby", magnus::wrap(class = "GitLabMergeRequestEventType"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitLabMergeRequestEventType {
    Detached,
    MergedResult,
    MergeTrain,
}

impl TryFrom<&str> for GitLabMergeRequestEventType {
    type Error = CIInfoParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "detached" => Ok(GitLabMergeRequestEventType::Detached),
            "merged_result" => Ok(GitLabMergeRequestEventType::MergedResult),
            "merge_train" => Ok(GitLabMergeRequestEventType::MergeTrain),
            _ => Err(CIInfoParseError::GitLabMergeRequestEventType),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "BranchClass"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchClass {
    PullRequest,
    ProtectedBranch,
    Merge,
    None,
}

impl
    From<(
        &str,
        Option<usize>,
        Option<GitLabMergeRequestEventType>,
        &[&str],
    )> for BranchClass
{
    fn from(
        value: (
            &str,
            Option<usize>,
            Option<GitLabMergeRequestEventType>,
            &[&str],
        ),
    ) -> Self {
        let (branch_name, pr_number, merge_request_event_type, stable_branches) = value;
        if branch_name.contains("trunk-merge/")
            || branch_name.contains("gh-readonly-queue/")
            || branch_name.contains("/gtmq_")
            || branch_name.starts_with("gtmq_")
            || merge_request_event_type
                .filter(|t| *t == GitLabMergeRequestEventType::MergeTrain)
                .is_some()
        {
            BranchClass::Merge
        } else if pr_number.is_some() {
            BranchClass::PullRequest
        } else if branch_name.starts_with("remotes/pull/") || branch_name.starts_with("pull/") {
            BranchClass::PullRequest
        } else if stable_branches.contains(&branch_name) {
            BranchClass::ProtectedBranch
        } else {
            BranchClass::None
        }
    }
}

impl ToString for BranchClass {
    fn to_string(&self) -> String {
        match self {
            BranchClass::PullRequest => "PR".to_string(),
            BranchClass::ProtectedBranch => "PB".to_string(),
            BranchClass::Merge => "MERGE".to_string(),
            BranchClass::None => "NONE".to_string(),
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

/// Characters that need to be percent-encoded in URL path segments
/// This includes CONTROLS plus characters that are special in URLs
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}');

/// URL-encode a string for use in a URL path segment
fn url_encode_path_segment(s: &str) -> String {
    utf8_percent_encode(s, PATH_SEGMENT_ENCODE_SET).to_string()
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

#[cfg(feature = "ruby")]
impl CIInfo {
    pub fn platform(&self) -> CIPlatform {
        self.platform
    }
    pub fn job_url(&self) -> Option<&str> {
        self.job_url.as_deref()
    }
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
    pub fn branch_class(&self) -> Option<BranchClass> {
        self.branch_class
    }
    pub fn pr_number(&self) -> Option<usize> {
        self.pr_number
    }
    pub fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }
    pub fn committer_name(&self) -> Option<&str> {
        self.committer_name.as_deref()
    }
    pub fn committer_email(&self) -> Option<&str> {
        self.committer_email.as_deref()
    }
    pub fn author_name(&self) -> Option<&str> {
        self.author_name.as_deref()
    }
    pub fn author_email(&self) -> Option<&str> {
        self.author_email.as_deref()
    }
    pub fn commit_message(&self) -> Option<&str> {
        self.commit_message.as_deref()
    }
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    pub fn workflow(&self) -> Option<&str> {
        self.workflow.as_deref()
    }
    pub fn job(&self) -> Option<&str> {
        self.job.as_deref()
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

    pub fn parse(
        &mut self,
        env_vars: &'a EnvVars,
        stable_branches: &'a [&str],
        repo: Option<&'a BundleRepo>,
    ) {
        self.parse_ci_platform(env_vars, stable_branches, repo);
        if let Some(ci_info) = &mut self.ci_info_parser {
            ci_info.parse();
        }
    }

    fn parse_ci_platform(
        &mut self,
        env_vars: &'a EnvVars,
        stable_branches: &'a [&str],
        repo: Option<&'a BundleRepo>,
    ) {
        self.ci_info_parser = Some(CIInfoParser::new(
            CIPlatform::from(env_vars),
            env_vars,
            stable_branches,
            repo,
        ));
    }
}

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let ci_platform = ruby.define_class("CIPlatform", ruby.class_object())?;
    ci_platform.define_method("to_s", magnus::method!(CIPlatform::to_string, 0))?;
    let branch_class = ruby.define_class("BranchClass", ruby.class_object())?;
    branch_class.define_method("to_s", magnus::method!(BranchClass::to_string, 0))?;
    let ci_info = ruby.define_class("CIInfo", ruby.class_object())?;
    ci_info.define_singleton_method("new", magnus::function!(CIInfo::new, 1))?;
    ci_info.define_method("platform", magnus::method!(CIInfo::platform, 0))?;
    ci_info.define_method("job_url", magnus::method!(CIInfo::job_url, 0))?;
    ci_info.define_method("branch", magnus::method!(CIInfo::branch, 0))?;
    ci_info.define_method("branch_class", magnus::method!(CIInfo::branch_class, 0))?;
    ci_info.define_method("pr_number", magnus::method!(CIInfo::pr_number, 0))?;
    ci_info.define_method("actor", magnus::method!(CIInfo::actor, 0))?;
    ci_info.define_method("committer_name", magnus::method!(CIInfo::committer_name, 0))?;
    ci_info.define_method(
        "committer_email",
        magnus::method!(CIInfo::committer_email, 0),
    )?;
    ci_info.define_method("author_name", magnus::method!(CIInfo::author_name, 0))?;
    ci_info.define_method("author_email", magnus::method!(CIInfo::author_email, 0))?;
    ci_info.define_method("commit_message", magnus::method!(CIInfo::commit_message, 0))?;
    ci_info.define_method("title", magnus::method!(CIInfo::title, 0))?;
    ci_info.define_method("workflow", magnus::method!(CIInfo::workflow, 0))?;
    ci_info.define_method("job", magnus::method!(CIInfo::job, 0))?;
    Ok(())
}

#[test]
fn unknown_is_lowest_ciplatform() {
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::Unknown),
        CIPlatform::Unknown,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::GitLabCI),
        CIPlatform::GitLabCI,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::Unknown),
        CIPlatform::Unknown,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::GitLabCI).prioritize(CIPlatform::Unknown),
        CIPlatform::GitLabCI,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::Unknown),
        CIPlatform::Custom,
    );
}

#[test]
fn custom_is_highest_ciplatform() {
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::GitLabCI),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::Unknown),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::GitLabCI).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
}

#[test]
fn other_ciplatforms_mid_tier() {
    pretty_assertions::assert_eq!(
        (CIPlatform::TravisCI).prioritize(CIPlatform::GitLabCI),
        CIPlatform::TravisCI,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::TravisCI).prioritize(CIPlatform::Custom),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::TravisCI).prioritize(CIPlatform::Unknown),
        CIPlatform::TravisCI,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::GitLabCI).prioritize(CIPlatform::TravisCI),
        CIPlatform::GitLabCI,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Custom).prioritize(CIPlatform::TravisCI),
        CIPlatform::Custom,
    );
    pretty_assertions::assert_eq!(
        (CIPlatform::Unknown).prioritize(CIPlatform::TravisCI),
        CIPlatform::TravisCI,
    );
}
