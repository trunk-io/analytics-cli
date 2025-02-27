use std::{collections::HashMap, path::Path};

use assert_cmd::Command;
use constants::{TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_PUBLIC_API_ADDRESS_ENV};

use crate::utils::CARGO_RUN;

const DEFAULT_JUNIT_PATHS: &str = "./*";

pub struct UploadArgs {
    pub repo_root: Option<String>,
    pub repo_url: Option<String>,
    pub repo_head_sha: Option<String>,
    pub repo_head_branch: Option<String>,
    pub repo_head_commit_epoch: Option<String>,
    pub tags: Option<Vec<String>>,
    pub print_files: Option<bool>,
    pub no_upload: Option<bool>,
    pub team: Option<String>,
    pub codeowners_path: Option<String>,
    pub use_quarantining: Option<bool>,
    pub disable_quarantining: Option<bool>,
    pub allow_empty_test_results: Option<bool>,
}

impl UploadArgs {
    pub fn empty() -> Self {
        UploadArgs {
            repo_root: None,
            repo_url: None,
            repo_head_sha: None,
            repo_head_branch: None,
            repo_head_commit_epoch: None,
            tags: None,
            print_files: None,
            no_upload: None,
            team: None,
            codeowners_path: None,
            use_quarantining: None,
            disable_quarantining: None,
            allow_empty_test_results: None,
        }
    }

    pub fn build_args(&self) -> Vec<String> {
        vec![
            String::from("--org-url-slug"),
            String::from("test-org"),
            String::from("--token"),
            String::from("test-token"),
        ]
        .into_iter()
        .chain(
            self.repo_root
                .clone()
                .into_iter()
                .flat_map(|repo_root: String| vec![String::from("--repo-root"), repo_root]),
        )
        .chain(
            self.repo_url
                .clone()
                .into_iter()
                .flat_map(|repo_url: String| vec![String::from("--repo-url"), repo_url]),
        )
        .chain(
            self.repo_head_branch
                .clone()
                .into_iter()
                .flat_map(|repo_head_branch: String| {
                    vec![String::from("--repo-head-branch"), repo_head_branch]
                }),
        )
        .chain(
            self.repo_head_sha
                .clone()
                .into_iter()
                .flat_map(|repo_head_sha: String| {
                    vec![String::from("--repo-head-sha"), repo_head_sha]
                }),
        )
        .chain(self.repo_head_commit_epoch.clone().into_iter().flat_map(
            |repo_head_commit_epoch: String| {
                vec![
                    String::from("--repo-head-commit-epoch"),
                    repo_head_commit_epoch,
                ]
            },
        ))
        .chain(self.tags.clone().into_iter().flat_map(|tags: Vec<String>| {
            if tags.is_empty() {
                vec![]
            } else {
                let mut args = vec![String::from("--tags")];
                args.extend(tags);
                args
            }
        }))
        .chain(self.print_files.into_iter().flat_map(|print_files: bool| {
            if print_files {
                vec![String::from("--print-files")]
            } else {
                vec![]
            }
        }))
        .chain(self.no_upload.into_iter().flat_map(|no_upload: bool| {
            if no_upload {
                vec![String::from("--repo-root")]
            } else {
                vec![]
            }
        }))
        .chain(
            self.team
                .clone()
                .into_iter()
                .flat_map(|team: String| vec![String::from("--team"), team]),
        )
        .chain(
            self.codeowners_path
                .clone()
                .into_iter()
                .flat_map(|codeowners_path: String| {
                    vec![String::from("--codeowners-path"), codeowners_path]
                }),
        )
        .chain(
            self.use_quarantining
                .into_iter()
                .flat_map(|use_quarantining: bool| {
                    if use_quarantining {
                        vec![String::from("--use-quarantining")]
                    } else {
                        vec![String::from("--use-quarantining=false")]
                    }
                }),
        )
        .chain(
            self.disable_quarantining
                .into_iter()
                .flat_map(|disable_quarantining: bool| {
                    if disable_quarantining {
                        vec![String::from("--disable-quarantining")]
                    } else {
                        vec![String::from("--disable-quarantining=false")]
                    }
                }),
        )
        .chain(self.allow_empty_test_results.into_iter().flat_map(
            |allow_empty_test_results: bool| {
                if allow_empty_test_results {
                    vec![String::from("--allow-empty-test-results")]
                } else {
                    vec![]
                }
            },
        ))
        .collect()
    }
}

pub enum CommandType {
    Upload {
        upload_args: UploadArgs,
        server_host: String,
    },
    Quarantine {
        upload_args: UploadArgs,
        server_host: String,
    },
    Test {
        upload_args: UploadArgs,
        command: Vec<String>,
        server_host: String,
    },
    Validate {
        show_warnings: Option<bool>,
        codeowners_path: Option<String>,
    },
}

impl CommandType {
    pub fn name(&self) -> String {
        match self {
            CommandType::Upload { .. } => String::from("upload"),
            CommandType::Quarantine { .. } => String::from("quarantine"),
            CommandType::Test { .. } => String::from("test"),
            CommandType::Validate { .. } => String::from("validate"),
        }
    }

    pub fn build_args(&self) -> Vec<String> {
        match self {
            CommandType::Upload { upload_args, .. } => upload_args.build_args(),
            CommandType::Quarantine { upload_args, .. } => upload_args.build_args(),
            CommandType::Test {
                upload_args,
                command,
                ..
            } => [upload_args.build_args(), command.clone()].concat(),
            CommandType::Validate {
                show_warnings,
                codeowners_path,
            } => show_warnings
                .iter()
                .flat_map(|show_warnings: &bool| {
                    if *show_warnings {
                        vec![String::from("--show-warnings")]
                    } else {
                        vec![]
                    }
                })
                .chain(codeowners_path.iter().flat_map(|codeowners_path: &String| {
                    vec![String::from("--codeowners-path"), codeowners_path.clone()]
                }))
                .collect(),
        }
    }

    pub fn build_envs(&self) -> HashMap<String, String> {
        match self {
            CommandType::Upload { server_host, .. } => HashMap::from([(
                String::from(TRUNK_PUBLIC_API_ADDRESS_ENV),
                server_host.clone(),
            )]),
            CommandType::Quarantine { server_host, .. } => HashMap::from([(
                String::from(TRUNK_PUBLIC_API_ADDRESS_ENV),
                server_host.clone(),
            )]),
            CommandType::Test { server_host, .. } => HashMap::from([(
                String::from(TRUNK_PUBLIC_API_ADDRESS_ENV),
                server_host.clone(),
            )]),
            CommandType::Validate { .. } => {
                let empty: HashMap<String, String> = HashMap::new();
                empty
            }
        }
    }

    pub fn use_quarantining(&mut self, new_flag: bool) -> &mut Self {
        match self {
            CommandType::Upload { upload_args, .. } => {
                upload_args.use_quarantining = Some(new_flag)
            }
            CommandType::Quarantine { upload_args, .. } => {
                upload_args.use_quarantining = Some(new_flag)
            }
            CommandType::Test { upload_args, .. } => upload_args.use_quarantining = Some(new_flag),
            CommandType::Validate { .. } => (),
        }
        self
    }

    pub fn disable_quarantining(&mut self, new_flag: bool) -> &mut Self {
        match self {
            CommandType::Upload { upload_args, .. } => {
                upload_args.disable_quarantining = Some(new_flag)
            }
            CommandType::Quarantine { upload_args, .. } => {
                upload_args.disable_quarantining = Some(new_flag)
            }
            CommandType::Test { upload_args, .. } => {
                upload_args.disable_quarantining = Some(new_flag)
            }
            CommandType::Validate { .. } => (),
        }
        self
    }

    pub fn print_files(&mut self, new_flag: bool) -> &mut Self {
        match self {
            CommandType::Upload { upload_args, .. } => upload_args.print_files = Some(new_flag),
            CommandType::Quarantine { upload_args, .. } => upload_args.print_files = Some(new_flag),
            CommandType::Test { upload_args, .. } => upload_args.print_files = Some(new_flag),
            CommandType::Validate { .. } => (),
        }
        self
    }

    pub fn repo_root(&mut self, new_value: &str) -> &mut Self {
        match self {
            CommandType::Upload { upload_args, .. } => {
                upload_args.repo_root = Some(String::from(new_value))
            }
            CommandType::Quarantine { upload_args, .. } => {
                upload_args.repo_root = Some(String::from(new_value))
            }
            CommandType::Test { upload_args, .. } => {
                upload_args.repo_root = Some(String::from(new_value))
            }
            CommandType::Validate { .. } => (),
        }
        self
    }
}

#[derive(Clone)]
pub enum PathsState {
    JunitPaths(String),
    BazelBepPath(String),
}

impl PathsState {
    pub fn build_args(&self) -> Vec<String> {
        match self {
            PathsState::JunitPaths(path) => vec![String::from("--junit-paths"), path.clone()],
            PathsState::BazelBepPath(path) => vec![String::from("--bazel-bep-path"), path.clone()],
        }
    }
}

pub struct CommandBuilder<'a> {
    command_type: CommandType,
    current_dir: &'a Path,
    paths_state: Option<PathsState>,
}

impl<'b> CommandBuilder<'b> {
    pub fn upload(
        current_dir: &'b Path,
        server_host: String,
        upload_args: Option<UploadArgs>,
    ) -> Self {
        CommandBuilder {
            command_type: CommandType::Upload {
                upload_args: upload_args.unwrap_or(UploadArgs::empty()),
                server_host,
            },
            current_dir,
            paths_state: None,
        }
    }

    pub fn quarantine(current_dir: &'b Path, server_host: String) -> Self {
        CommandBuilder {
            command_type: CommandType::Quarantine {
                upload_args: UploadArgs::empty(),
                server_host,
            },
            current_dir,
            paths_state: None,
        }
    }

    pub fn test(current_dir: &'b Path, server_host: String, command: Vec<String>) -> Self {
        CommandBuilder {
            command_type: CommandType::Test {
                upload_args: UploadArgs::empty(),
                command,
                server_host,
            },
            current_dir,
            paths_state: None,
        }
    }

    pub fn validate(current_dir: &'b Path) -> Self {
        CommandBuilder {
            command_type: CommandType::Validate {
                show_warnings: None,
                codeowners_path: None,
            },
            current_dir,
            paths_state: None,
        }
    }

    pub fn junit_paths(&mut self, new_paths: &str) -> &mut Self {
        self.paths_state = Some(PathsState::JunitPaths(String::from(new_paths)));
        self
    }

    pub fn bazel_bep_path(&mut self, new_paths: &str) -> &mut Self {
        self.paths_state = Some(PathsState::BazelBepPath(String::from(new_paths)));
        self
    }

    pub fn use_quarantining(&mut self, new_flag: bool) -> &mut Self {
        self.command_type.use_quarantining(new_flag);
        self
    }

    pub fn disable_quarantining(&mut self, new_flag: bool) -> &mut Self {
        self.command_type.disable_quarantining(new_flag);
        self
    }

    pub fn print_files(&mut self, new_flag: bool) -> &mut Self {
        self.command_type.print_files(new_flag);
        self
    }

    pub fn repo_root(&mut self, new_value: &str) -> &mut Self {
        self.command_type.repo_root(new_value);
        self
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(CARGO_RUN.path());
        let args = self.build_args();
        let envs = self.build_envs();
        command.current_dir(self.current_dir).envs(envs).args(args);
        command
    }

    pub fn build_args(&self) -> Vec<String> {
        let paths_args = self
            .paths_state
            .clone()
            .map(|paths_state: PathsState| paths_state.build_args())
            .unwrap_or(vec![
                String::from("--junit-paths"),
                String::from(DEFAULT_JUNIT_PATHS),
            ]);

        [self.command_type.name()]
            .into_iter()
            .chain(paths_args)
            .chain(self.command_type.build_args())
            .collect()
    }

    fn build_envs(&self) -> HashMap<String, String> {
        let mut base_env: HashMap<String, String> = HashMap::from([
            (
                String::from(TRUNK_API_CLIENT_RETRY_COUNT_ENV),
                String::from("0"),
            ),
            (String::from("CI"), String::from("1")),
            (String::from("GITHUB_JOB"), String::from("test-job")),
        ]);
        base_env.extend(self.command_type.build_envs());
        base_env
    }
}
