use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Deprecation,
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
