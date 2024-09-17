use const_format::concatcp;
use fancy_regex::{escape, Regex};
use lazy_static::lazy_static;

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/app/models/user.rb#L1025
pub const REFERENCE_PREFIX: &str = "@";

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/app/models/namespace.rb#L39
const NUMBER_OF_ANCESTORS_ALLOWED: usize = 20;
/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/app/models/namespace.rb#L45
const URL_MAX_LENGTH: usize = 255;
/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L133
const PATH_START_CHAR: &str = r"[a-zA-Z0-9_\.]";
/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L134
const PATH_REGEX_STR: &str = concatcp!(
    PATH_START_CHAR,
    r"[a-zA-Z0-9_\-\.]",
    "{0,",
    URL_MAX_LENGTH - 1,
    "}",
);
/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L135
const NAMESPACE_FORMAT_REGEX_JS: &str = concatcp!(PATH_REGEX_STR, r"[a-zA-Z0-9_\-]|[a-zA-Z0-9_]");

lazy_static! {
  /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L137
  static ref NO_SUFFIX_REGEX: Regex = Regex::new(r"(?<!\.git|\.atom)").unwrap();

  /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L138
  static ref NAMESPACE_FORMAT_REGEX: Regex = Regex::new(&format!(r"(?:{}){}", NAMESPACE_FORMAT_REGEX_JS, NO_SUFFIX_REGEX.as_str())).unwrap();

  /// https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/lib/gitlab/path_regex.rb#L140
  static ref FULL_NAMESPACE_FORMAT_REGEX: Regex = Regex::new(&format!(r"({}/){{0,{}}}{}", NAMESPACE_FORMAT_REGEX.as_str(), NUMBER_OF_ANCESTORS_ALLOWED, NAMESPACE_FORMAT_REGEX.as_str())).unwrap();

  /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/df0d6654bb540cf7321e297554acd8adb3e2e3a4/app/models/user.rb#L1030
  pub static ref REFERENCE_PATTERN: Regex = Regex::new(&format!(r"(?<!\w){}(?<user>{})", escape(REFERENCE_PREFIX), FULL_NAMESPACE_FORMAT_REGEX.as_str())).unwrap();
}
