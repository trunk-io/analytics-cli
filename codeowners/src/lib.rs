mod codeowners;
mod github;
mod gitlab;
mod traits;

pub use codeowners::{
    BindingsOwners, BindingsOwnersAndSource, CodeOwners, CodeOwnersFile, Owners, OwnersSource,
    associate_codeowners, associate_codeowners_multithreaded, flatten_code_owners,
};
pub use github::{BindingsGitHubOwners, GitHubOwner, GitHubOwners};
pub use gitlab::{BindingsGitLabOwners, GitLabOwner, GitLabOwners};
pub use traits::{FromPath, FromReader, OwnersOfPath};
