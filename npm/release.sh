#!/usr/bin/env bash
# Publish @glitchtip/cli + platform packages to npm.
#
# Usage:
#   npm/release.sh <version> <artifacts-dir>
#
# <artifacts-dir> must contain the binaries built by .gitlab-ci.yml:
#   glitchtip-cli-linux-x86_64
#   glitchtip-cli-linux-arm64
#   glitchtip-cli-macos-arm64
#   glitchtip-cli-windows-x86_64.exe
#
# Requires NPM_TOKEN in env (or `npm login` already done locally).

set -euo pipefail

VERSION="${1:?version required, e.g. 1.0.1}"
ARTIFACTS="${2:?artifacts dir required}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# artifact filename -> npm platform dir + bin name
declare -A PLATFORMS=(
  ["glitchtip-cli-linux-x86_64"]="linux-x64:glitchtip-cli"
  ["glitchtip-cli-linux-arm64"]="linux-arm64:glitchtip-cli"
  ["glitchtip-cli-macos-arm64"]="darwin-arm64:glitchtip-cli"
  ["glitchtip-cli-windows-x86_64.exe"]="win32-x64:glitchtip-cli.exe"
)

# Stage binaries into each platform package and publish.
for artifact in "${!PLATFORMS[@]}"; do
  IFS=":" read -r platform_dir bin_name <<< "${PLATFORMS[$artifact]}"
  pkg_dir="${SCRIPT_DIR}/platforms/${platform_dir}"
  src="${ARTIFACTS}/${artifact}"

  [ -f "${src}" ] || { echo "missing artifact: ${src}" >&2; exit 1; }

  mkdir -p "${pkg_dir}/bin"
  cp "${src}" "${pkg_dir}/bin/${bin_name}"
  chmod +x "${pkg_dir}/bin/${bin_name}"

  # stamp version
  node -e "
    const fs = require('fs');
    const p = '${pkg_dir}/package.json';
    const j = JSON.parse(fs.readFileSync(p, 'utf8'));
    j.version = '${VERSION}';
    fs.writeFileSync(p, JSON.stringify(j, null, 2) + '\n');
  "

  echo "Publishing $(node -p "require('${pkg_dir}/package.json').name")@${VERSION}"
  (cd "${pkg_dir}" && npm publish --access public)
done

# Root package: stamp version + pin optionalDependencies to this exact version,
# then publish last (so npm install picks up the matching platform pkg).
CLI_DIR="${SCRIPT_DIR}/cli"
cp "${ROOT_DIR}/README.md" "${CLI_DIR}/README.md"
cp "${ROOT_DIR}/LICENSE" "${CLI_DIR}/LICENSE"

node -e "
  const fs = require('fs');
  const p = '${CLI_DIR}/package.json';
  const j = JSON.parse(fs.readFileSync(p, 'utf8'));
  j.version = '${VERSION}';
  for (const dep of Object.keys(j.optionalDependencies)) {
    j.optionalDependencies[dep] = '${VERSION}';
  }
  fs.writeFileSync(p, JSON.stringify(j, null, 2) + '\n');
"

echo "Publishing @glitchtip/cli@${VERSION}"
(cd "${CLI_DIR}" && npm publish --access public)
