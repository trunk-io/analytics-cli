use crate::types::CustomTag;

pub const MAX_KEY_LEN: usize = 32;
pub const MAX_VAL_LEN: usize = 1024 * 8;

pub fn parse_custom_tags(tags: &[String]) -> anyhow::Result<Vec<CustomTag>> {
    let parsed = tags.iter()
        .filter(|tag_str| !tag_str.trim().is_empty())
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

            if key.is_empty() || value.is_empty() {
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

    Ok(parsed)
}
