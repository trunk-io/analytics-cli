use crate::types::CustomTag;

pub const MAX_KEY_LEN: usize = 32;
pub const MAX_VAL_LEN: usize = 1024 * 8;

pub fn print_status_code_help(status: reqwest::StatusCode) -> String {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => {
            "Your Trunk token may be incorrect - \
             find it on the Trunk app (Settings -> \
             Manage Organization -> Organization \
             API Token -> View)."
        }
        reqwest::StatusCode::NOT_FOUND => {
            "Your Trunk organization URL \
             slug may be incorrect - find \
             it on the Trunk app (Settings \
             -> Manage Organization -> \
             Organization Slug)."
        }
        _ => "For more help, contact us at https://slack.trunk.io/",
    }
    .to_string()
}

pub fn from_non_empty_or_default<R, F: Fn(String) -> R>(
    s: Option<String>,
    default: R,
    from_non_empty: F,
) -> R {
    if let Some(s) = s {
        if s.trim().len() > 0 {
            return from_non_empty(s);
        }
    }
    default
}

pub fn parse_custom_tags(tags: &[String]) -> anyhow::Result<Vec<CustomTag>> {
    let parsed = tags.iter()
        .filter(|tag_str| tag_str.trim().len() > 0)
        .map(|tag_str| {
            let parts = tag_str.split('=').collect::<Vec<&str>>();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Invalid custom tag format. Expected exactly 2 parts: {:?} (tags: {:?})",
                    parts,
                    tags
                ));
            }

            let key = parts[0].trim().to_owned();
            let value = parts[1].trim().to_owned();

            if key.len() == 0 || value.len() == 0 {
                return Err(anyhow::anyhow!(
                    "Invalid custom tag format. Key/Value is empty: {:?}",
                    tags
                ));
            }

            if key.len() > MAX_KEY_LEN || value.len() > MAX_VAL_LEN {
                return Err(anyhow::anyhow!(
                    "Invalid custom tag format. Key/Value is too long: {:?}. Max key len: {}, max value len: {}",
                    tags,
                    MAX_KEY_LEN,
                    MAX_VAL_LEN
                ));
            }

            Ok(CustomTag { key, value })
        })
        .collect::<anyhow::Result<Vec<CustomTag>>>()?;

    return Ok(parsed);
}
