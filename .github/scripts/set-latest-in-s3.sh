#!/usr/bin/env bash
# Point releases/analytics-cli/prod/channel.json at an already-published version. This is the
# only mutable object in the prefix; consumers read `latest` from it.
#
# Usage: set-latest-in-s3.sh <version>
set -euo pipefail

bucket="${BUCKET:-trunk-releases}"
prefix="${PREFIX:-releases/analytics-cli/prod}"
version="${1:?usage: set-latest-in-s3.sh <version>}"

if [[ ! ${version} =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
	echo "::error::'${version}' is not a semver version"
	exit 1
fi

manifest_key="${prefix}/${version}/manifest.json"
if ! output="$(aws s3api head-object --bucket "${bucket}" --key "${manifest_key}" 2>&1)"; then
	if [[ ${output} == *"Not Found"* || ${output} == *"404"* ]]; then
		echo "::error::no s3://${bucket}/${manifest_key}; ${version} was never published (or only partially). Publish it first (mirror_release_to_s3.yml)."
	else
		echo "::error::head-object s3://${bucket}/${manifest_key} failed: ${output}"
	fi
	exit 1
fi

jq -nc --arg v "${version}" '{latest: $v}' |
	aws s3 cp - "s3://${bucket}/${prefix}/channel.json" --only-show-errors \
		--content-type "application/json" --cache-control "max-age=60"
echo "https://trunk.io/${prefix}/channel.json now points at ${version}"
