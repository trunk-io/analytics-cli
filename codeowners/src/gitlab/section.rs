/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/section.rb
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub optional: bool,
    pub approvals: usize,
    pub default_owners: String,
}

impl Section {
    pub const DEFAULT: &'static str = "codeowners";

    pub fn new(
        name: String,
        optional: Option<bool>,
        approvals: Option<usize>,
        default_owners: Option<String>,
    ) -> Self {
        Self {
            name,
            optional: optional.unwrap_or_default(),
            approvals: approvals.unwrap_or_default(),
            default_owners: default_owners
                .map(|s| String::from(s.trim()))
                .unwrap_or_default(),
        }
    }
}
