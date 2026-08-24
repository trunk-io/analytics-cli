use sha2::{Digest, Sha256};
use uuid::Uuid;

fn generate_checksum_uuid(values: Vec<&str>) -> String {
    let info_id_input: String = values.join("#");
    Uuid::new_v5(&Uuid::NAMESPACE_URL, info_id_input.as_bytes()).to_string()
}

/// Deterministic, globally-unique public id for a test case in a test collection.
///
/// A collection test case is only unique by the `(test_collection_id, repo_id, test_case_id)`
/// tuple: `test_case_id` is opaque to the server and `--no-repo` deliberately shares one id
/// across a collection's repos (nil `repo_id`). This hashes that whole tuple into one id, so it
/// inherits the tuple's semantics wholesale -- including the `--no-repo` collapse.
///
/// The contract is FROZEN and enforced by the pinned golden values in this file's tests. The
/// server computes the same id from the same inputs, so changing any step here changes which ids
/// exist for every test case already reported.
///
///   1. the three UUIDs as canonical lowercase hyphenated text (`Display`),
///   2. joined `"{test_collection_id}#{repo_id}#{test_case_id}"` (`#` mirrors `gen_info_id`),
///   3. SHA-256, first 16 bytes,
///   4. stamped RFC 9562 UUIDv8 (exactly what `Uuid::new_v8` does).
///
/// Step 4 is load-bearing, not cosmetic: consumers validate this id with a UUID matcher that
/// enforces version 1-8 plus variant `[89ab]`, which an unstamped truncated hash fails ~87.5% of
/// the time. v8 also visibly distinguishes this id from the v4/v5 ids alongside it.
pub fn gen_test_case_guid(test_collection_id: Uuid, repo_id: Uuid, test_case_id: Uuid) -> Uuid {
    let msg = format!("{test_collection_id}#{repo_id}#{test_case_id}");
    let digest = Sha256::digest(msg.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::new_v8(bytes)
}

// trunk-ignore(clippy/too_many_arguments)
pub fn gen_info_id_base(
    org_url_slug: &str,
    repo_full_name: &str,
    file: Option<&str>,
    classname: Option<&str>,
    parent_fact_path: Option<&str>,
    name: Option<&str>,
    info_id: Option<&str>,
    variant: &str,
) -> String {
    let mut base_values = vec![
        org_url_slug,
        repo_full_name,
        file.unwrap_or(""),
        classname.unwrap_or(""),
        parent_fact_path.unwrap_or(""),
        name.unwrap_or(""),
        "JUNIT_TESTCASE", // Compatibility with legacy code
    ];
    let id_and_variant_values = vec![info_id.unwrap_or(""), variant];
    let mut alt_values = vec![org_url_slug, repo_full_name, info_id.unwrap_or("")];
    let mut has_variant = false;

    if !variant.is_empty() {
        base_values.push(variant);
        alt_values.push(variant);
        has_variant = true;
    }

    if let Some(info_id) = info_id {
        if !info_id.is_empty() {
            if info_id.starts_with("trunk:") {
                return generate_checksum_uuid(alt_values);
            } else if let Ok(uuid) = Uuid::parse_str(info_id) {
                if uuid.get_version_num() == 5 {
                    if has_variant {
                        return generate_checksum_uuid(id_and_variant_values);
                    } else {
                        return info_id.to_string();
                    }
                }
            }
        }
    }
    generate_checksum_uuid(base_values)
}

// trunk-ignore(clippy/too_many_arguments)
pub fn gen_info_id(
    org_url_slug: &str,
    repo_full_name: &str,
    file: Option<&str>,
    classname: Option<&str>,
    parent_fact_path: Option<&str>,
    name: Option<&str>,
    info_id: Option<&str>,
    variant: &str,
) -> String {
    let id = gen_info_id_base(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        variant,
    );
    if variant.is_empty() {
        id
    } else {
        gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            Some(id.as_str()),
            variant,
        )
    }
}

#[cfg(test)]
#[cfg(feature = "bindings")]
mod tests {
    use uuid::Uuid;

    use crate::meta::id::{gen_info_id, gen_info_id_base, gen_test_case_guid};

    #[cfg(feature = "bindings")]
    #[test]
    fn test_variant_wrapper_doesnt_change_non_variant_case() {
        let org_url_slug = "example_org";
        let repo_full_name = "example_repo";
        let file = Some("src/lib.rs");
        let classname = Some("ExampleClass");
        let parent_fact_path = Some("parent/fact/path");
        let name = Some("example_name");
        let info_id = None;
        let variant = "";

        let result = gen_info_id(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        let base_result = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        let expected = "06cb6db5-f807-5198-b072-af67a0636f8a";
        assert_eq!(result, expected);
        assert_eq!(base_result, expected);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_variant_wrapper_does_change_variant_case() {
        let org_url_slug = "example_org";
        let repo_full_name = "example_repo";
        let file = Some("src/lib.rs");
        let classname = Some("ExampleClass");
        let parent_fact_path = Some("parent/fact/path");
        let name = Some("example_name");
        let info_id = None;
        let variant = "unix";

        let result = gen_info_id(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        let base_result = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        let expected = "1bf61475-b542-5faf-aa85-e66a691257a3";
        assert_eq!(result, expected);
        assert_ne!(base_result, expected);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_info_id_trunk() {
        let org_url_slug = "example_org";
        let repo_full_name = "example_repo";
        let file = Some("src/lib.rs");
        let classname = Some("ExampleClass");
        let parent_fact_path = Some("parent/fact/path");
        let name = Some("example_name");
        let info_id = Some("trunk:12345");
        let variant = "unix";

        let result = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        assert_eq!(result, "4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b");

        // Run again to ensure deterministic output
        let result_again = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );
        assert_eq!(result_again, result);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_info_id_real_staging_test() {
        // This test legitimately exists - checking to see that this code generates the
        // expected ID.
        let result = gen_info_id_base(
            "trunk-staging-org",
            "github.com/trunk-io/trunk",
            None,
            Some("modules/settings/repoName/__tests__/ticketing-integration.vitest.tsx"),
            Some("modules/settings/repoName/__tests__/ticketing-integration.vitest.tsx"),
            Some("Ticketing Integration > should allow you to select a ticketing system"),
            None,
            "",
        );

        // https://app.trunk-staging.io/trunk-staging-org/flaky-tests/test/3f507aef-e834-523b-a8ad-edaba6b137be?repo=trunk-io%2Ftrunk
        assert_eq!(result, "3f507aef-e834-523b-a8ad-edaba6b137be")
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_info_id_existing_v5_uuid() {
        let org_url_slug = "example_org";
        let repo_full_name = "example_repo";
        let file = Some("src/lib.rs");
        let classname = Some("ExampleClass");
        let parent_fact_path = Some("parent/fact/path");
        let name = Some("example_name");
        let info_id = Some("a6e84936-3ee9-57d5-b041-ae124896f654");
        let variant = "";

        let result = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        assert_eq!(result, info_id.map_or(String::new(), |id| id.to_string()));

        // Run again to ensure deterministic output
        let result_again = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );
        assert_eq!(result_again, result);

        // Check that adding a variant does generate a new ID
        let variant = "unix";
        let result_with_variant = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );
        assert_ne!(
            result_with_variant,
            info_id.map_or(String::new(), |id| id.to_string())
        );
        assert_eq!(result_with_variant, "8057218b-95e4-5373-afbe-c366d4058615");
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_info_id_no_existing_v5_uuid() {
        let org_url_slug = "example_org";
        let repo_full_name = "example_repo";
        let file = Some("src/lib.rs");
        let classname = Some("ExampleClass");
        let parent_fact_path = Some("parent/fact/path");
        let name = Some("example_name");
        let info_id = None;
        let variant = "unix";

        let result = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );

        assert_eq!(result, "c869cb93-66e2-516d-a0ea-15ff4b413c3f");

        // Run again to ensure deterministic output
        let result_again = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id,
            variant,
        );
        assert_eq!(result_again, result);

        // Test with v4 UUID
        let info_id_v4 = Some("08e1c642-3a55-45cf-8bf9-b9d0b21785dd"); // v4 UUID
        let result_v4 = gen_info_id_base(
            org_url_slug,
            repo_full_name,
            file,
            classname,
            parent_fact_path,
            name,
            info_id_v4,
            variant,
        );
        assert_ne!(
            result_v4,
            info_id_v4.map_or(String::new(), |id| id.to_string())
        );
        assert_eq!(result_v4, result_again);
    }

    // ----------------------------------------------------------------------------------
    // gen_test_case_guid -- FROZEN contract.
    //
    // These two vectors ARE the contract. They are pinned in every binding's test suite, and
    // server-side consumers pin them too; the golden values are what keep those copies honest.
    // A change here is a change to which ids exist for every test case already reported.
    // ----------------------------------------------------------------------------------

    const GOLDEN_COLLECTION_ID: &str = "018f6d3a-6f2e-4c4a-9b1e-2f3a4b5c6d7e";
    const GOLDEN_REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";
    const GOLDEN_TEST_CASE_ID: &str = "88e5353c-190c-5dce-9d06-0e66c3e062b1";

    /// `--repo` (the ordinary case): a real repo UUID participates in the hash.
    const GOLDEN_GUID_WITH_REPO: &str = "bfeebcf4-72d1-887d-8bcd-788d0dec7f97";
    /// `--no-repo`: the nil repo UUID, so one guid per collection across repos.
    const GOLDEN_GUID_NO_REPO: &str = "943a80af-66b0-84bb-ad01-56b3b72fe363";

    fn golden_guid(repo_id: Uuid) -> Uuid {
        gen_test_case_guid(
            Uuid::parse_str(GOLDEN_COLLECTION_ID).unwrap(),
            repo_id,
            Uuid::parse_str(GOLDEN_TEST_CASE_ID).unwrap(),
        )
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_test_case_guid_golden_with_repo() {
        let result = golden_guid(Uuid::parse_str(GOLDEN_REPO_ID).unwrap());
        assert_eq!(result.to_string(), GOLDEN_GUID_WITH_REPO);

        // Run again to ensure deterministic output
        let result_again = golden_guid(Uuid::parse_str(GOLDEN_REPO_ID).unwrap());
        assert_eq!(result_again, result);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_test_case_guid_golden_no_repo() {
        let result = golden_guid(Uuid::nil());
        assert_eq!(result.to_string(), GOLDEN_GUID_NO_REPO);

        // Run again to ensure deterministic output
        let result_again = golden_guid(Uuid::nil());
        assert_eq!(result_again, result);
    }

    /// The contract hashes the *canonical lowercase* rendering, so uppercase input text must
    /// normalize to the same guid. `Uuid`'s `Display` is what guarantees this.
    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_test_case_guid_normalizes_uppercase_inputs() {
        let result = gen_test_case_guid(
            Uuid::parse_str(&GOLDEN_COLLECTION_ID.to_uppercase()).unwrap(),
            Uuid::parse_str(&GOLDEN_REPO_ID.to_uppercase()).unwrap(),
            Uuid::parse_str(&GOLDEN_TEST_CASE_ID.to_uppercase()).unwrap(),
        );
        assert_eq!(result.to_string(), GOLDEN_GUID_WITH_REPO);
    }

    /// Consumers validate this id with a UUID matcher enforcing version 1-8 and variant `[89ab]`,
    /// so an unstamped hash would be rejected ~87.5% of the time. Assert the stamp directly --
    /// far cheaper to catch here than at the API edge.
    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_test_case_guid_is_stamped_v8() {
        for repo_id in [Uuid::parse_str(GOLDEN_REPO_ID).unwrap(), Uuid::nil()] {
            let guid = golden_guid(repo_id);
            assert_eq!(guid.get_version_num(), 8);
            // RFC 9562 variant: the high two bits of byte 8 are 0b10.
            assert_eq!(guid.as_bytes()[8] & 0xC0, 0x80);
        }
    }

    /// The two collision vectors the tuple encodes must stay distinct: a real repo and the nil
    /// repo are different tuples, so they are different guids.
    #[cfg(feature = "bindings")]
    #[test]
    fn test_gen_test_case_guid_repo_id_changes_the_guid() {
        assert_ne!(
            golden_guid(Uuid::parse_str(GOLDEN_REPO_ID).unwrap()),
            golden_guid(Uuid::nil())
        );
    }
}
