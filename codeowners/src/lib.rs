mod codeowners;
mod github;
mod gitlab;
mod traits;

pub use codeowners::{CodeOwners, Owners};
pub use github::{GitHubOwner, GitHubOwners};
pub use gitlab::{GitLabOwner, GitLabOwners};
pub use traits::{FromPath, FromReader, OwnersOfPath};
