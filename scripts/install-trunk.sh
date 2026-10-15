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
