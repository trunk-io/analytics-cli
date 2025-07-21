mod codeowners;
mod github;
mod gitlab;
mod traits;

pub use codeowners::{
    associate_codeowners, associate_codeowners_multithreaded, flatten_code_owners, BindingsOwners,
    CodeOwners, Owners,
};
pub use github::{BindingsGitHubOwners, GitHubOwner, GitHubOwners};
pub use gitlab::{BindingsGitLabOwners, GitLabOwner, GitLabOwners};
pub use traits::{FromPath, FromReader, OwnersOfPath};
