use assert_cmd::{Command, cargo};
use pretty_assertions::assert_eq;
use trunk_analytics_cli::summary_output::json;

#[test]
fn prints_the_summary_output_schema() {
    let output = Command::new(cargo::cargo_bin!())
        .arg("summary-output-schema")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
        serde_json::to_value(json::Report::schema()).unwrap()
    );
}
