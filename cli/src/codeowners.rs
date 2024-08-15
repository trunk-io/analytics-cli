use std::{
    fs::File,
    path::{Path, PathBuf},
};

use codeowners::Owners;

use crate::constants::CODEOWNERS_LOCATIONS;

#[derive(Default, Debug)]
pub struct CodeOwners {
    pub path: PathBuf,
    pub file: Option<File>,
    pub owners: Option<Owners>,
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
            let file = Result::ok(File::open(&path));
            let owners = file.as_ref().and_then(|f| {
                let owners_result = codeowners::from_reader(f);
                if let Err(ref err) = owners_result {
                    log::error!(
                        "Found CODEOWNERS file `{}`, but couldn't parse it: {}",
                        path.to_string_lossy(),
                        err
                    );
                }
                Result::ok(owners_result)
            });
            Self { path, file, owners }
        })
    }
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
