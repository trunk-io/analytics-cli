import json
import pathlib
import re
import subprocess


def main():
    result = subprocess.run(
        ["xcrun", "xcresulttool", "get", "test-results", "tests", "--schema"],
        capture_output=True,
        text=True,
    ).stdout.replace("#/schemas", "#/$defs")
    json_schema = json.loads(result)

    json_schema["$defs"] = json_schema["schemas"]
    del json_schema["schemas"]

    pathlib.Path(__file__).parent.resolve().joinpath(
        "./xcrun-xcresulttool-get-test-results-tests-json-schema.json"
    ).write_text(json.dumps(json_schema, indent=2))


if __name__ == "__main__":
    main()
