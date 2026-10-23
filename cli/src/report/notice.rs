use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoticeKind {
    /// A deprecated flag was passed.
    Deprecation,
    /// Part of the Bazel BEP file couldn't be parsed.
    BepParseError,
}

impl Notice {
    pub fn deprecation<T: Into<String>>(message: T) -> Self {
        Self {
            kind: NoticeKind::Deprecation,
            message: message.into(),
        }
    }

    pub fn bep_parse_error<T: Into<String>>(message: T) -> Self {
        Self {
            kind: NoticeKind::BepParseError,
            message: message.into(),
        }
    }
}
