use std::collections::HashMap;

use bundle::{BundleMetaBaseProps, TestCollectionProps};
use context::repo::BundleRepo;
use serde_json::json;

fn build_base_props(test_collection: Option<TestCollectionProps>) -> BundleMetaBaseProps {
    BundleMetaBaseProps {
        version: String::from("1"),
        cli_version: String::from("trunk-analytics-cli test"),
        org: String::from("test-org"),
        test_collection,
        repo: BundleRepo::default(),
        bundle_upload_id: String::from("bundle-upload-id"),
        tags: vec![],
        file_sets: vec![],
        envs: HashMap::new(),
        upload_time_epoch: 0,
        test_command: None,
        os_info: None,
        quarantined_tests: vec![],
        codeowners: None,
        use_uncloned_repo: None,
    }
}

#[test]
fn serializes_test_collection_props_as_flattened_fields() {
    let base_props = build_base_props(Some(TestCollectionProps {
        short_id: String::from("tc_123"),
        bundle_meta_id: String::from("82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc"),
        bundle_meta_created_at: String::from("2026-05-10T12:34:56.000Z"),
    }));

    let value = serde_json::to_value(&base_props).unwrap();
    let object = value.as_object().unwrap();

    assert_eq!(object.get("test_collection"), None);
    assert_eq!(
        object.get("test_collection_short_id"),
        Some(&json!("tc_123"))
    );
    assert_eq!(
        object.get("test_collection_bundle_meta_id"),
        Some(&json!("82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc"))
    );
    assert_eq!(
        object.get("test_collection_bundle_meta_created_at"),
        Some(&json!("2026-05-10T12:34:56.000Z"))
    );

    let reparsed: BundleMetaBaseProps = serde_json::from_value(value).unwrap();
    assert_eq!(reparsed.test_collection, base_props.test_collection);
}
