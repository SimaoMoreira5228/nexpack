#!/usr/bin/env bash
set -euo pipefail

REPO="${GITHUB_REPOSITORY:-SimaoMoreira5228/nexpack}"
README="README.md"

latest_tag=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | jq -r '.tag_name // empty')

if [ -z "$latest_tag" ]; then
  echo "no releases found"
  exit 0
fi

badges="current release: [${latest_tag}](https://github.com/${REPO}/releases/tag/${latest_tag})

downloads:
- [nxpk-linux-x86_64](https://github.com/${REPO}/releases/download/${latest_tag}/nxpk-linux-x86_64)
- [nexpackd-linux-x86_64](https://github.com/${REPO}/releases/download/${latest_tag}/nexpackd-linux-x86_64)
- [nexpack-source.tar.gz](https://github.com/${REPO}/releases/download/${latest_tag}/nexpack-source.tar.gz)
- [sha256sums.txt](https://github.com/${REPO}/releases/download/${latest_tag}/sha256sums.txt)"

if grep -q "current release:" "$README" 2>/dev/null; then
  awk -v badges="$badges" '
    /^current release:/ { print badges; in_block=1; next }
    /^$/ && in_block { in_block=0 }
    !in_block { print }
  ' "$README" > "${README}.tmp" && mv "${README}.tmp" "$README"
else
  echo "" >> "$README"
  echo "---" >> "$README"
  echo "" >> "$README"
  echo "$badges" >> "$README"
fi

echo "release index updated to ${latest_tag}"
