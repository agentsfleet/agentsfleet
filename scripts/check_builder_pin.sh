#!/usr/bin/env bash
# check_builder_pin.sh — the musl builder's version is written in exactly one
# place, and the workflows derive it rather than repeating it.
#
# THE FAILURE THIS EXISTS TO PREVENT.
#
# `playbooks/operations/ci_rust_images/versions.env` pins RUST_VERSION and
# ALPINE_SERIES. `make/build.mk` derives BUILDER_IMAGE from it. If a workflow
# pastes the resulting tag as a literal instead, a versions.env bump moves the
# local build and leaves Continuous Integration (CI) on the old image — and
# nothing fails, because the old tag still exists and still pulls. CI simply
# compiles on a toolchain the repository has moved off, which is the "second
# compiler nobody chose" that versions.env's own comment exists to prevent.
#
# WHY THIS ASSERTS ABSENCE RATHER THAN EQUALITY.
#
# Checking that a pasted literal MATCHES versions.env sounds stronger and is
# weaker: it passes right up until the bump, which is the only moment it
# mattered. Asserting no literal exists makes the drift unreachable — there is
# no second place for the version to be wrong.
#
# Exit: 0 clean · 1 a workflow pins a literal builder tag · 2 usage error.

set -euo pipefail

WORKFLOW_DIR="${1:-.github/workflows}"
readonly WORKFLOW_DIR
readonly IMAGE="ghcr.io/agentsfleet/ci-rust-alpine"

if [ ! -d "$WORKFLOW_DIR" ]; then
  echo "usage: check_builder_pin.sh [workflow-dir]" >&2
  exit 2
fi

# A literal is the image name followed by a version-looking tag. The derived
# form is `$IMAGE:$RUST_VERSION-alpine$ALPINE_SERIES`, whose character after
# the colon is `$` — so anchoring on a DIGIT separates them without a
# second spelling of the tag shape.
if hits="$(grep -rnE "${IMAGE}:[0-9]" "$WORKFLOW_DIR" 2>/dev/null)"; then
  echo "✗ [gh-actions] a workflow pins a literal builder tag:"
  printf '%s\n' "$hits" | sed 's/^/    /'
  echo "  Derive it from playbooks/operations/ci_rust_images/versions.env instead,"
  echo "  the way make/build.mk does:"
  echo "    RUST_VERSION=\"\$(sed -n 's/^RUST_VERSION=//p' playbooks/operations/ci_rust_images/versions.env)\""
  exit 1
fi

echo "✓ [gh-actions] no workflow pins a literal builder tag"
