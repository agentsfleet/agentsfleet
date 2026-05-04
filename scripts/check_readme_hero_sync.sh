#!/usr/bin/env bash
# test_readme_hero_sync — assert the hero paragraph in this repo's README.md
# is byte-identical to ~/Projects/.github/profile/README.md (the org-profile
# landing page on github.com/usezombie). Spec §4.5 requires both surfaces to
# carry the same hero so the launch positioning is consistent.
#
# Skips with a warning when the org-profile README is not present locally
# (CI environments without the .github repo cloned alongside).
#
#     bash scripts/check_readme_hero_sync.sh

set -euo pipefail

ROOT_README="README.md"
PROFILE_README="$HOME/Projects/.github/profile/README.md"

if [ ! -f "$PROFILE_README" ]; then
  echo "SKIP: $PROFILE_README not present (org-profile repo not cloned alongside)"
  exit 0
fi

# The hero is the bolded paragraph immediately after the </picture> block.
# Extract: first non-empty line starting with "**" after the </picture> close.
extract_hero() {
  local file="$1"
  awk '
    /<\/picture>/      { in_picture=0; next }
    /<picture>/        { in_picture=1; next }
    in_picture         { next }
    /^[[:space:]]*$/   { if (started) exit; next }
    /^\*\*/            { print; started=1; next }
    started            { exit }
  ' "$file"
}

ROOT_HERO=$(extract_hero "$ROOT_README")
PROFILE_HERO=$(extract_hero "$PROFILE_README")

if [ -z "$ROOT_HERO" ]; then
  echo "FAIL: hero paragraph not found in $ROOT_README" >&2
  exit 1
fi

if [ -z "$PROFILE_HERO" ]; then
  echo "FAIL: hero paragraph not found in $PROFILE_README" >&2
  exit 1
fi

if [ "$ROOT_HERO" = "$PROFILE_HERO" ]; then
  echo "OK: hero paragraph byte-identical across both READMEs ($(echo "$ROOT_HERO" | wc -c | tr -d ' ') bytes)"
  exit 0
fi

echo "FAIL: hero paragraph differs between READMEs" >&2
echo "--- $ROOT_README ---" >&2
echo "$ROOT_HERO" >&2
echo "--- $PROFILE_README ---" >&2
echo "$PROFILE_HERO" >&2
exit 1
