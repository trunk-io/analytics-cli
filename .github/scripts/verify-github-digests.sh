#!/usr/bin/env bash
# GitHub records asset digests only since June 2025 (0.10.0-beta.0 on); older assets pass unchecked.
# Usage: verify-github-digests.sh <tag> <asset-dir>
set -euo pipefail

tag="${1:?usage: verify-github-digests.sh <tag> <asset-dir>}"
asset_dir="${2:?usage: verify-github-digests.sh <tag> <asset-dir>}"

declare -A digests
while IFS=$'\t' read -r name digest; do
	digests["${name}"]="${digest}"
done < <(gh api "repos/{owner}/{repo}/releases/tags/${tag}" \
	--jq '.assets[] | [.name, (.digest // "")] | @tsv')

verified=0
unverified=0
shopt -s nullglob
for asset in "${asset_dir}"/*.tar.gz "${asset_dir}"/*.zip; do
	name="$(basename "${asset}")"
	expected="${digests[${name}]-}"
	if [[ -z ${expected} ]]; then
		unverified=$((unverified + 1))
		continue
	fi
	actual="sha256:$(sha256sum "${asset}" | cut -d' ' -f1)"
	if [[ ${actual} != "${expected}" ]]; then
		echo "::error::${tag}/${name}: downloaded ${actual}, GitHub recorded ${expected}"
		exit 1
	fi
	verified=$((verified + 1))
done

echo "${tag}: ${verified} asset(s) match GitHub's digest, ${unverified} have no GitHub digest"
