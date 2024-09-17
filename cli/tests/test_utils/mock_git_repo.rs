use std::io::Write;
use std::path::Path;

pub const TEST_BRANCH: &str = "trunk/test";
pub const TEST_ORIGIN: &str = "https://github.com/trunk-io/analytics-cli.git";
pub const TEST_FILE: &str = "file.txt";

pub fn setup_repo_with_commit<T: AsRef<Path>>(root: T) -> anyhow::Result<()> {
    let repo = git2::Repository::init(root).expect("failed to init repo");
    repo.remote_set_url("origin", TEST_ORIGIN)?;

    let file_path = std::path::Path::new(&repo.workdir().unwrap()).join(TEST_FILE);
    let mut file = std::fs::File::create(&file_path).expect("Could not create file");
    writeln!(file, "test content").expect("Could not write to file");

    // Add the new file to the index
    let mut index = repo.index()?;
    index.add_path(std::path::Path::new(TEST_FILE))?;
    index.write()?;

    // Create a new commit
    let oid = index.write_tree()?;
    let signature = git2::Signature::now("Your Name", "your.email@example.com")?;
    let tree = repo.find_tree(oid)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )?;

    // Create and checkout a new branch
    let obj = repo.revparse_single("HEAD")?;
    repo.branch(TEST_BRANCH, &obj.as_commit().unwrap(), false)?;
    repo.set_head(format!("refs/heads/{}", TEST_BRANCH).as_str())?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;

    Ok(())
}
