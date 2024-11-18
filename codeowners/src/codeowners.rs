use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
#[cfg(feature = "wasm")]
use wasm_bindgen::{
    convert::{FromWasmAbi, IntoWasmAbi, OptionFromWasmAbi, OptionIntoWasmAbi},
    describe::WasmDescribe,
};

use constants::CODEOWNERS_LOCATIONS;

use crate::{github::GitHubOwners, gitlab::GitLabOwners, traits::FromReader};

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CodeOwners {
    pub path: PathBuf,
    #[serde(skip_serializing, skip_deserializing)]
    pub owners: Option<Owners>,
}

// NOTE(Tyler): Presently, with wasm we only check for presence of codeowners object. Custom conversion
// is needed to fully support parsing it.
#[cfg(feature = "wasm")]
impl WasmDescribe for CodeOwners {
    fn describe() {
        js_sys::Object::describe()
    }
}
#[cfg(feature = "wasm")]
impl IntoWasmAbi for CodeOwners {
    type Abi = u32;
    fn into_abi(self) -> Self::Abi {
        let map = js_sys::Object::new();
        map.into_abi()
    }
}
#[cfg(feature = "wasm")]
impl FromWasmAbi for CodeOwners {
    type Abi = u32;
    unsafe fn from_abi(_js: Self::Abi) -> Self {
        CodeOwners::default()
    }
}
#[cfg(feature = "wasm")]
impl OptionIntoWasmAbi for CodeOwners {
    fn none() -> Self::Abi {
        wasm_bindgen::JsValue::UNDEFINED.into_abi()
    }
}
#[cfg(feature = "wasm")]
impl OptionFromWasmAbi for CodeOwners {
    fn is_none(_abi: &Self::Abi) -> bool {
        true
    }
}

impl CodeOwners {
    pub fn find_file<T: AsRef<Path>, U: AsRef<Path>>(
        repo_root: T,
        codeowners_path_cli_option: &Option<U>,
    ) -> Option<Self> {
        let cli_option_location = codeowners_path_cli_option
            .as_slice()
            .iter()
            .map(|path_str| -> &Path { path_str.as_ref() });
        let default_locations = CODEOWNERS_LOCATIONS
            .iter()
            .map(|path_str| -> &Path { path_str.as_ref() });
        let mut all_locations = cli_option_location.chain(default_locations);

        let codeowners_path =
            all_locations.find_map(|location| locate_codeowners(&repo_root, location));

        codeowners_path.map(|path| {
            let owners_result = File::open(&path)
                .map_err(anyhow::Error::from)
                .and_then(|file| GitHubOwners::from_reader(&file).map(Owners::GitHubOwners))
                .or_else(|_| {
                    File::open(&path)
                        .map_err(anyhow::Error::from)
                        .and_then(|file| GitLabOwners::from_reader(&file).map(Owners::GitLabOwners))
                });

            if let Err(ref err) = owners_result {
                log::error!(
                    "Found CODEOWNERS file `{}`, but couldn't parse it: {}",
                    path.to_string_lossy(),
                    err
                );
            }

            let owners = Result::ok(owners_result);
            Self { path, owners }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Owners {
    GitHubOwners(GitHubOwners),
    GitLabOwners(GitLabOwners),
}

const CODEOWNERS: &str = "CODEOWNERS";

fn locate_codeowners<T, U>(repo_root: T, location: U) -> Option<PathBuf>
where
    T: AsRef<Path>,
    U: AsRef<Path>,
{
    let file = repo_root.as_ref().join(location).join(CODEOWNERS);
    if file.is_file() {
        Some(file)
    } else {
        None
    }
}
