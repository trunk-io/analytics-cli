# Source

Based on [crate](https://crates.io/crates/bazel-bep), [repo](https://github.com/ChristianBelloni/bazel-bep)

Original protos found [here](https://github.com/bazelbuild/bazel/blob/master/src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto)

## Updating Protos for New Bazel Versions

### Current Situation

The upstream vendor crate (`bazel-bep`) has not been updated in over a year. As a result, we maintain our own copy of the proto definitions to ensure compatibility with newer Bazel versions. This is a short-term solution while we transition away from parsing JSON to using proto output directly from Bazel, which will help prevent breakage in the future.

### Why This Matters

When Bazel releases a new version, the Build Event Protocol (BEP) proto definitions may change. If our proto definitions are out of date, we may encounter:

- Parsing failures when processing BEP files from newer Bazel versions
- Missing or incorrectly parsed fields
- Compatibility issues that break our analytics pipeline

### How to Update Protos

When a new Bazel version is released, follow these steps to update the proto definitions:

1. **Identify the Bazel version and proto location**

   - Check the [Bazel releases page](https://github.com/bazelbuild/bazel/releases) for the latest version
   - The proto files are located in the Bazel repository at:
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/action_cache.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/command_line.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/failure_details.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/package_load_metrics.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/publish_build_event.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_events.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/option_filters.proto`
     - `src/main/java/com/google/devtools/build/lib/buildeventstream/proto/invocation_policy.proto`

2. **Download the updated proto files**

   - Clone or update the Bazel repository: `git clone https://github.com/bazelbuild/bazel.git`
   - Check out the specific Bazel version tag: `git checkout <version-tag>`
   - Copy the proto files from the Bazel repository to `bazel-bep/proto/`
   - Also copy any Google API proto files from `third_party/googleapis/google/api/` if they've changed

3. **Update dependencies if needed**

   - Check if the proto changes require updates to `Cargo.toml` dependencies
   - Review any breaking changes in `prost`, `tonic-build`, or related crates

4. **Test the changes**

   - Run `cargo build` to ensure the protos compile correctly
   - Run tests with BEP files from the new Bazel version
   - Verify that both JSON and binary BEP parsing still work correctly

5. **Document the Bazel version**
   - Update this README or add a comment indicating which Bazel version the protos are compatible with
   - Consider adding a test that verifies compatibility with specific Bazel versions

### Long-term Plan

The current approach of manually maintaining proto definitions is not scalable. We should:

1. **Move to proto output from Bazel**: Instead of parsing JSON, enforce using only Bazel's native proto output format to reduce compatibility issues
2. **Automate proto updates**: Consider creating a script or CI job that automatically checks for Bazel updates and updates the proto files
3. **Version compatibility testing**: Establish a testing strategy that verifies compatibility with multiple Bazel versions

### Related Files

- Proto definitions: `bazel-bep/proto/`
- Build script: `bazel-bep/build.rs`
- Parser implementations: `context/src/bazel_bep/parser.rs` (JSON) and `context/src/bazel_bep/binary_parser.rs` (binary)
