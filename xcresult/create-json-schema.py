import json
import pathlib

# trunk-ignore(bandit/B404)
import subprocess


def main():
    # trunk-ignore(bandit/B603,bandit/B607)
    result = subprocess.run(
        ["xcrun", "xcresulttool", "help", "get", "test-results", "tests"],
        capture_output=True,
        text=True,
    ).stdout.replace("#/schemas", "#/$defs")
    json_schema_lines = result.split("\n")[3:148]
    json_schema = json.loads("\n".join(json_schema_lines))

    json_schema["$defs"] = json_schema["schemas"]
    del json_schema["schemas"]
    json_schema["$ref"] = "#/$defs/Tests"

    pathlib.Path(__file__).parent.resolve().joinpath(
        "./xcrun-xcresulttool-get-test-results-tests-json-schema.json"
    ).write_text(json.dumps(json_schema, indent=2))


if __name__ == "__main__":
    main()
