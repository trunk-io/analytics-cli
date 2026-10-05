#!/usr/bin/env bash
# Publish one release's assets to releases/analytics-cli/prod/<version>/ in the trunk-releases
# bucket (served at https://trunk.io/releases/analytics-cli/prod/). Never moves channel.json:
# a published version is a prerelease until promote_release.yml points latest at it.
#
# Usage: publish-release-to-s3.sh [--skip-complete] <version> <asset-dir>
#
# Every *.tar.gz / *.zip in <asset-dir> is uploaded under its own filename. Objects are
# immutable: an asset already in the bucket is skipped if its recorded sha256 matches and is an
# error if it differs. manifest.json goes up last, so its presence means the version is
# complete. With --skip-complete (backfills), a version whose manifest already exists is a
# no-op; without it, that is an error.
set -euo pipefail

bucket="${BUCKET:-trunk-releases}"
prefix="${PREFIX:-releases/analytics-cli/prod}"

skip_complete=false
if [[ ${1-} == "--skip-complete" ]]; then
	skip_complete=true
	shift
fi
version="${1:?usage: publish-release-to-s3.sh [--skip-complete] <version> <asset-dir>}"
asset_dir="${2:?usage: publish-release-to-s3.sh [--skip-complete] <version> <asset-dir>}"

if [[ ! ${version} =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
	echo "::error::'${version}' is not a semver version"
	exit 1
fi

# Prints the object's recorded sha256, or nothing when the key does not exist. Any failure other
# than a 404 aborts: without s3:ListBucket a missing key reads as 403, which must not be taken
# for "absent".
recorded_sha256() {
	local key="$1" output
	if output="$(aws s3api head-object --bucket "${bucket}" --key "${key}" \
		--query 'Metadata.sha256' --output text 2>&1)"; then
		echo "${output}"
	elif [[ ${output} != *"Not Found"* && ${output} != *"404"* ]]; then
		echo "::error::head-object s3://${bucket}/${key} failed: ${output}" >&2
		return 1
	fi
}

version_prefix="${prefix}/${version}"
manifest_key="${version_prefix}/manifest.json"

manifest_sha="$(recorded_sha256 "${manifest_key}")"
if [[ -n ${manifest_sha} ]]; then
	if [[ ${skip_complete} == true ]]; then
		echo "${version} is already published (s3://${bucket}/${manifest_key}); nothing to do"
		exit 0
	fi
	echo "::error::${version} is already published (s3://${bucket}/${manifest_key}); released versions are immutable"
	exit 1
fi

shopt -s nullglob
assets=("${asset_dir}"/*.tar.gz "${asset_dir}"/*.zip)
if [[ ${#assets[@]} -eq 0 ]]; then
	echo "::error::no *.tar.gz or *.zip assets in ${asset_dir}"
	exit 1
fi

manifest="$(jq -n --arg v "${version}" '{version: $v, artifacts: {}}')"
for asset in "${assets[@]}"; do
	name="$(basename "${asset}")"
	key="${version_prefix}/${name}"
	sha="$(sha256sum "${asset}" | cut -d' ' -f1)"
	case "${name}" in
	*.zip) content_type="application/zip" ;;
	*) content_type="application/gzip" ;;
	esac

	existing="$(recorded_sha256 "${key}")"
	if [[ -z ${existing} ]]; then
		aws s3 cp "${asset}" "s3://${bucket}/${key}" --only-show-errors \
			--content-type "${content_type}" \
			--metadata "sha256=${sha},release-version=${version}"
		echo "uploaded ${key}"
	elif [[ ${existing} == "${sha}" ]]; then
		echo "already uploaded ${key}"
	else
		echo "::error::s3://${bucket}/${key} already exists with sha256 ${existing}, not ${sha}; released artifacts are immutable"
		exit 1
	fi

	manifest="$(jq --arg n "${name}" --arg s "${sha}" '.artifacts[$n] = {sha256: $s}' <<<"${manifest}")"
done

manifest_file="$(mktemp)"
jq . <<<"${manifest}" >"${manifest_file}"
aws s3 cp "${manifest_file}" "s3://${bucket}/${manifest_key}" --only-show-errors \
	--content-type "application/json" \
	--metadata "sha256=$(sha256sum "${manifest_file}" | cut -d' ' -f1),release-version=${version}"
rm -f "${manifest_file}"
echo "published ${version} to https://trunk.io/${version_prefix}/ (${#assets[@]} artifacts)"
