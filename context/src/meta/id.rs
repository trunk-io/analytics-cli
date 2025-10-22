use uuid::Uuid;

fn generate_checksum_uuid(values: Vec<&str>) -> String {
    let info_id_input: String = values.join("#");
    Uuid::new_v5(&Uuid::NAMESPACE_URL, info_id_input.as_bytes()).to_string()
}

fn generate_info_id(
    info_id: Option<&str>,
    base_values: Vec<&str>,
    alt_values: Vec<&str>,
    id_and_variant_values: Vec<&str>,
    has_variant: bool,
) -> String {
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

    generate_info_id(
        info_id,
        base_values,
        alt_values,
        id_and_variant_values,
        has_variant,
    )
}

// trunk-ignore(clippy/too_many_arguments)
pub fn generate_info_id_variant_wrapper(
    org_url_slug: &str,
    repo_full_name: &str,
    file: Option<&str>,
    classname: Option<&str>,
    parent_fact_path: Option<&str>,
    name: Option<&str>,
    info_id: Option<&str>,
    variant: &str,
) -> String {
    let id = gen_info_id(
        org_url_slug,
        repo_full_name,
        file,
        classname,
        parent_fact_path,
        name,
        info_id,
        variant,
    );
    if (variant.is_empty() || info_id.is_some()) {
        id
    } else {
        gen_info_id(
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
    use crate::meta::id::gen_info_id;

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

        assert_eq!(result, "4392f63c-8dc9-5cec-bbdc-e7b90c2e5a6b");

        // Run again to ensure deterministic output
        let result_again = gen_info_id(
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
        let result = gen_info_id(
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

        assert_eq!(result, info_id.map_or(String::new(), |id| id.to_string()));

        // Run again to ensure deterministic output
        let result_again = gen_info_id(
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
        let result_with_variant = gen_info_id(
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

        assert_eq!(result, "c869cb93-66e2-516d-a0ea-15ff4b413c3f");

        // Run again to ensure deterministic output
        let result_again = gen_info_id(
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
        let result_v4 = gen_info_id(
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
}
