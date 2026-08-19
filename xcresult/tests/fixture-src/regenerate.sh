#!/usr/bin/env bash
#
# Capture the .xcresult fixtures in `xcresult/tests/data/` from the scenario
# packages next to this script. Requires macOS and Xcode.
#
#   ./regenerate.sh                            # every scenario
#   ./regenerate.sh objc-xctest                # just one
#   FIXTURE_WORK_DIR=/tmp/elsewhere ./regenerate.sh
#
# Absolute paths are baked into a bundle at capture time and end up in the
# expected JUnit XML, so the scenarios are copied to a fixed working directory
# (`/tmp/xcresult-fixtures` by default) and built there. Regenerating from a
# different directory rewrites every path in the expected output — see README.md.

set -euo pipefail

FIXTURE_SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$(cd "${FIXTURE_SRC_DIR}/../data" && pwd)"
WORK_DIR="${FIXTURE_WORK_DIR:-/tmp/xcresult-fixtures}"

ALL_SCENARIOS=(
	dependency-raises-failure
	in-repo-helper-raises-failure
	crash-in-dependency
	objc-xctest
	toplevel-swift-testing
)

# scenario -> the package name, which is both the xcodebuild scheme prefix and the
# name of the captured bundle.
package_name() {
	case "$1" in
	dependency-raises-failure) echo DependencyRaisesFailure ;;
	in-repo-helper-raises-failure) echo InRepoHelperRaisesFailure ;;
	crash-in-dependency) echo CrashInDependency ;;
	objc-xctest) echo ObjcXCTest ;;
	toplevel-swift-testing) echo ToplevelSwiftTesting ;;
	*)
		echo "unknown scenario: $1" >&2
		exit 1
		;;
	esac
}

regenerate() {
	local scenario="$1"
	local package
	package="$(package_name "${scenario}")"
	local scenario_work_dir="${WORK_DIR}/${scenario}"
	local bundle="${scenario_work_dir}/${package}.xcresult"

	echo "==> ${scenario}"

	rm -rf "${scenario_work_dir}"
	mkdir -p "${WORK_DIR}"
	cp -R "${FIXTURE_SRC_DIR}/${scenario}" "${scenario_work_dir}"

	# A `.package(url: "./Dependency", ...)` is only checked out into
	# `SourcePackages/checkouts` if it is a git repository, and the fixture is
	# pointless unless it is: that checkout path is the whole shape being
	# reproduced. The repository is created in the working copy so the checked-in
	# sources stay a plain directory.
	if [[ -d "${scenario_work_dir}/Dependency" ]]; then
		git -C "${scenario_work_dir}/Dependency" init -q
		git -C "${scenario_work_dir}/Dependency" add -A
		git -C "${scenario_work_dir}/Dependency" \
			-c user.email=fixtures@trunk.io -c user.name=fixtures \
			commit -qm "xcresult fixture dependency"
		git -C "${scenario_work_dir}/Dependency" branch -qM main
	fi

	# Tests are meant to fail here, so xcodebuild exits nonzero on a good run.
	(
		cd "${scenario_work_dir}"
		xcodebuild test \
			-scheme "${package}-Package" \
			-destination 'platform=macOS' \
			-derivedDataPath DerivedData \
			-resultBundlePath "${package}.xcresult" \
			>xcodebuild.log 2>&1
	) || true

	if [[ ! -d ${bundle} ]]; then
		echo "  no bundle at ${bundle}; see ${scenario_work_dir}/xcodebuild.log" >&2
		return 1
	fi

	# Xcode 26 writes ~95MB of unreferenced symbolication data into every bundle,
	# so pruning is what makes these checkable into git at all. Both xcresulttool
	# APIs the crate calls are compared before and after to prove the prune is
	# invisible to consumers.
	local before_legacy="${scenario_work_dir}/before-legacy.json"
	local before_modern="${scenario_work_dir}/before-modern.json"
	"${FIXTURE_SRC_DIR}/dump-failure-summaries.py" "${bundle}" >"${before_legacy}"
	xcrun xcresulttool get test-results tests --path "${bundle}" --format json \
		>"${before_modern}"

	"${FIXTURE_SRC_DIR}/prune-bundle.py" "${bundle}"

	# Derived from the bundle, so it is written to the working directory rather
	# than checked in; `dump-failure-summaries.py` regenerates it on demand.
	local dump="${scenario_work_dir}/${scenario}.failure-summaries.json"
	"${FIXTURE_SRC_DIR}/dump-failure-summaries.py" "${bundle}" >"${dump}"
	diff -q "${before_legacy}" "${dump}" >/dev/null ||
		{
			echo "  pruning changed the legacy failure summaries" >&2
			return 1
		}
	xcrun xcresulttool get test-results tests --path "${bundle}" --format json |
		diff -q "${before_modern}" - >/dev/null ||
		{
			echo "  pruning changed the test-results output" >&2
			return 1
		}

	"${FIXTURE_SRC_DIR}/verify-failure-summaries.py" "${scenario}" "${dump}"

	tar -czf "${DATA_DIR}/test-${scenario}.xcresult.tar.gz" \
		-C "${scenario_work_dir}" "${package}.xcresult"
	echo "  wrote ${DATA_DIR}/test-${scenario}.xcresult.tar.gz"
}

scenarios=("${@-}")
if [[ -z ${scenarios[0]} ]]; then
	scenarios=("${ALL_SCENARIOS[@]}")
fi

for scenario in "${scenarios[@]}"; do
	regenerate "${scenario}"
done

cat <<'EOF'

Bundles regenerated. The expected JUnit XML in `xcresult/tests/data/` still has
the old absolute paths baked in; update it with

    cargo test -p xcresult

and check every `file` attribute by hand before saving the new output — see
README.md for what each scenario must report.
EOF
