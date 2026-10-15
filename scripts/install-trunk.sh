#!/usr/bin/env bash
set -euo pipefail

# The CI runners are Linux x86_64. Verify the pinned upstream release.
version=0.21.14
sha256=f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b
archive="${RUNNER_TEMP:?}/trunk-linux.tar.gz"
destination="${RUNNER_TEMP}/trunk-bin"
curl --fail --location --retry 3 \
  "https://github.com/trunk-rs/trunk/releases/download/v${version}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
  --output "$archive"
printf '%s  %s\n' "$sha256" "$archive" | sha256sum --check --strict
mkdir -p "$destination"
tar -xzf "$archive" -C "$destination" trunk
echo "$destination" >> "${GITHUB_PATH:?}"

# Keep this version in sync with [tools].wasm_opt in Trunk.toml.
binaryen_version=133
binaryen_sha256=2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489
binaryen_archive="${RUNNER_TEMP}/binaryen-linux.tar.gz"
curl --fail --location --retry 3 \
  "https://github.com/WebAssembly/binaryen/releases/download/version_${binaryen_version}/binaryen-version_${binaryen_version}-x86_64-linux.tar.gz" \
  --output "$binaryen_archive"
printf '%s  %s\n' "$binaryen_sha256" "$binaryen_archive" | sha256sum --check --strict
tar -xzf "$binaryen_archive" -C "$RUNNER_TEMP"
echo "${RUNNER_TEMP}/binaryen-version_${binaryen_version}/bin" >> "$GITHUB_PATH"
