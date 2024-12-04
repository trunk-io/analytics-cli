#[cfg(feature = "ruby")]
use magnus::{value::ReprValue, Module, Object};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::string_safety::safe_truncate_string;

use super::EnvVars;

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum EnvParseError {
    #[error("no env vars passed")]
    EnvVarsEmpty,
    #[error("could not parse CI platform from env vars")]
    CIPlatform,
}

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
}

#[cfg(feature = "ruby")]
impl CIPlatform {
    pub fn to_string(&self) -> &str {
        match self {
            CIPlatform::GitHubActions => "GITHUB_ACTIONS",
            CIPlatform::JenkinsPipeline => "JENKINS_PIPELINE",
            CIPlatform::CircleCI => "CIRCLECI",
            CIPlatform::Buildkite => "BUILDKITE",
            CIPlatform::Semaphore => "SEMAPHORE",
            CIPlatform::TravisCI => "TRAVIS_CI",
            CIPlatform::Webappio => "WEBAPPIO",
            CIPlatform::AWSCodeBuild => "AWS_CODEBUILD",
            CIPlatform::BitbucketPipelines => "BITBUCKET",
            CIPlatform::AzurePipelines => "AZURE_PIPELINES",
            CIPlatform::GitLabCI => "GITLAB_CI",
            CIPlatform::Drone => "DRONE",
        }
    }
}

#[cfg(feature = "ruby")]
impl magnus::TryConvert for CIPlatform {
    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {
        let val: i32 = val.funcall("to_i", ())?;
        match val {
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
            // TODO
            _ => Ok(CIPlatform::GitHubActions),
        }
    }
}

impl TryFrom<&str> for CIPlatform {
    type Error = EnvParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let ci_platform = match value {
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
            _ => return Err(EnvParseError::CIPlatform),
        };

        Ok(ci_platform)
    }
}

impl TryFrom<&EnvVars> for CIPlatform {
    type Error = EnvParseError;

    fn try_from(value: &EnvVars) -> Result<Self, Self::Error> {
        let mut ci_platform = Err(EnvParseError::EnvVarsEmpty);
        for (key, ..) in value.iter() {
            ci_platform = CIPlatform::try_from(key.as_str());
            if ci_platform.is_ok() {
                break;
            }
        }
        ci_platform
    }
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum CIInfoParseError {
    #[error("could not parse branch class")]
    BranchClass,
}

const MAX_BRANCH_NAME_SIZE: usize = 1000;

#[derive(Debug, Clone)]
pub struct CIInfoParser<'a> {
    errors: Vec<CIInfoParseError>,
    ci_info: CIInfo,
    env_vars: &'a EnvVars,
}

impl<'a> CIInfoParser<'a> {
    pub fn new(platform: CIPlatform, env_vars: &'a EnvVars) -> Self {
        Self {
            errors: Vec::new(),
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

    pub fn parse(&mut self) -> anyhow::Result<()> {
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
            | CIPlatform::AzurePipelines => {
                // TODO(TRUNK-12908): Switch to using a crate for parsing the CI platform and related env vars
                // TODO(TRUNK-12909): parse more platforms
            }
        };
        self.clean_branch();
        self.parse_branch_class();
        Ok(())
    }

    fn clean_branch(&mut self) {
        if let Some(branch) = &mut self.ci_info.branch {
            let new_branch = branch
                .replace("refs/heads/", "")
                .replace("refs/", "")
                .replace("origin/", "");

            *branch = String::from(safe_truncate_string::<MAX_BRANCH_NAME_SIZE, _>(&new_branch));
        }
    }

    fn parse_branch_class(&mut self) {
        if let Some(branch) = &self.ci_info.branch {
            match BranchClass::try_from((branch.as_str(), self.ci_info.pr_number)) {
                Ok(branch_class) => {
                    self.ci_info.branch_class = Some(branch_class);
                }
                Err(err) => {
                    self.errors.push(err);
                }
            }
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
#[cfg_attr(feature = "ruby", magnus::wrap(class = "BranchClass"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchClass {
    PullRequest,
    ProtectedBranch,
    Merge,
}

impl TryFrom<(&str, Option<usize>)> for BranchClass {
    type Error = CIInfoParseError;

    fn try_from(value: (&str, Option<usize>)) -> Result<Self, Self::Error> {
        let (branch_name, pr_number) = value;
        if pr_number.is_some() {
            return Ok(BranchClass::PullRequest);
        }
        if branch_name.starts_with("remotes/pull/") || branch_name.starts_with("pull/") {
            Ok(BranchClass::PullRequest)
        } else if matches!(branch_name, "master" | "main") {
            Ok(BranchClass::ProtectedBranch)
        } else if branch_name.contains("/trunk-merge/") {
            Ok(BranchClass::Merge)
        } else {
            Err(CIInfoParseError::BranchClass)
        }
    }
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

// unfortunately magnus requires explicit getters and setters for the object
// to be accessible from Ruby
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
    errors: Vec<EnvParseError>,
    ci_info_parser: Option<CIInfoParser<'a>>,
}

impl<'a> EnvParser<'a> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn errors(&self) -> &Vec<EnvParseError> {
        &self.errors
    }

    pub fn ci_info_parser(&self) -> &Option<CIInfoParser> {
        &self.ci_info_parser
    }

    pub fn into_ci_info_parser(self) -> Option<CIInfoParser<'a>> {
        self.ci_info_parser
    }

    pub fn parse(&mut self, env_vars: &'a EnvVars) -> anyhow::Result<()> {
        self.parse_ci_platform(env_vars);
        if let Some(ci_info) = &mut self.ci_info_parser {
            ci_info.parse()?;
        }
        Ok(())
    }

    fn parse_ci_platform(&mut self, env_vars: &'a EnvVars) {
        match CIPlatform::try_from(env_vars) {
            Ok(ci_platform) => {
                self.ci_info_parser = Some(CIInfoParser::new(ci_platform, &env_vars));
            }
            Err(err) => {
                self.errors.push(err);
            }
        }
    }
}

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let ci_platform = ruby.define_class("CIPlatform", ruby.class_object())?;
    ci_platform.define_method("to_s", magnus::method!(CIPlatform::to_string, 0))?;
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
