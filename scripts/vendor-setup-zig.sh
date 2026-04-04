#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR="${1:-$HOME/Projects/oss/setup-zig}"
TARGET_DIR="$(cd "$(dirname "$0")/.." && pwd)/.github/actions/setup-zig"

if [ ! -d "$SOURCE_DIR" ]; then
  echo "source directory not found: $SOURCE_DIR" >&2
  exit 1
fi

mkdir -p "$(dirname "$TARGET_DIR")"
rsync -a --delete --exclude '.git' "$SOURCE_DIR/" "$TARGET_DIR/"

echo "Vendored setup-zig from $SOURCE_DIR -> $TARGET_DIR"
