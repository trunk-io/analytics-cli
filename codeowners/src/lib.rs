mod codeowners;
mod github;
mod gitlab;
mod traits;

pub use codeowners::{BindingsOwners, CodeOwners, Owners, CODEOWNERS};
pub use github::{BindingsGitHubOwners, GitHubOwner, GitHubOwners};
pub use gitlab::{BindingsGitLabOwners, GitLabOwner, GitLabOwners};
pub use traits::{FromPath, FromReader, OwnersOfPath};
