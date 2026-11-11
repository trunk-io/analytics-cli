#!/usr/bin/env bash
# Point releases/analytics-cli/prod/channel.json and channel-<major>.json at an already-published
# version. These are the only mutable objects in the prefix. Each pointer only moves forward
# (unless ALLOW_DOWNGRADE=true), so a 0.x backport after 1.0.0 moves channel-0.json alone.
#
# Usage: set-latest-in-s3.sh <version>
set -euo pipefail
shopt -s inherit_errexit

bucket="${BUCKET:-trunk-releases}"
prefix="${PREFIX:-releases/analytics-cli/prod}"
allow_downgrade="${ALLOW_DOWNGRADE:-false}"
version="${1:?usage: set-latest-in-s3.sh <version>}"

if [[ ! ${version} =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
	echo "::error::'${version}' is not a semver version"
	exit 1
fi
major="${version%%.*}"

manifest_key="${prefix}/${version}/manifest.json"
if ! output="$(aws s3api head-object --bucket "${bucket}" --key "${manifest_key}" 2>&1)"; then
	if [[ ${output} == *"Not Found"* || ${output} == *"404"* ]]; then
		echo "::error::no s3://${bucket}/${manifest_key}; ${version} was never published (or only partially). Publish it first (mirror_release_to_s3.yml)."
	else
		echo "::error::head-object s3://${bucket}/${manifest_key} failed: ${output}"
	fi
	exit 1
fi

current_latest() {
	local key="${prefix}/$1" body output
	body="$(mktemp)"
	if ! output="$(aws s3api get-object --bucket "${bucket}" --key "${key}" "${body}" 2>&1)"; then
		rm -f "${body}"
		if [[ ${output} == *"NoSuchKey"* || ${output} == *"Not Found"* || ${output} == *"404"* ]]; then
			return 0
		fi
		echo "::error::get-object s3://${bucket}/${key} failed: ${output}" >&2
		return 1
	fi
	jq -r '.latest // empty' "${body}"
	rm -f "${body}"
}

at_or_after() {
	[[ -z $2 ]] || [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | tail -n1)" == "$1" ]]
}

move_pointer() {
	local name="$1" current
	current="$(current_latest "${name}")"
	if [[ ${allow_downgrade} != "true" ]] && ! at_or_after "${version}" "${current}"; then
		echo "https://trunk.io/${prefix}/${name} stays at ${current}: ${version} is older (set ALLOW_DOWNGRADE=true to move it back)" >&2
		echo kept
		return 0
	fi
	jq -nc --arg v "${version}" '{latest: $v}' |
		aws s3 cp - "s3://${bucket}/${prefix}/${name}" --only-show-errors \
			--content-type "application/json" --cache-control "max-age=60"
	echo "https://trunk.io/${prefix}/${name} now points at ${version}" >&2
	echo moved
}

major_result="$(move_pointer "channel-${major}.json")"
global_result="$(move_pointer "channel.json")"

if [[ ${major_result} == kept && ${global_result} == kept ]]; then
	echo "::warning::${version} moved no pointer; both already point at a newer version"
fi
if [[ -n ${GITHUB_OUTPUT-} ]]; then
	if [[ ${global_result} == moved ]]; then
		echo "global_moved=true" >>"${GITHUB_OUTPUT}"
	else
		echo "global_moved=false" >>"${GITHUB_OUTPUT}"
	fi
fi
