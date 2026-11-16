# =============================================================================
# BUILD — version manifests, the builder image tag and the toolbox image
# =============================================================================

.PHONY: sync-version check-version toolbox-image

# The commit, computed once and EXPORTED.
# `rustd/crates/daemon/afd_api/build.rs` reads it for the commit `/healthz`
# reports, `scripts/build-candidate.sh` hands it to the builder container, and
# the bench lanes record it as a result's revision, so all three name one
# value. The export matters because a container build context carries no
# `.git`, so the build script cannot find the commit by itself.
GIT_COMMIT := $(if $(GITHUB_SHA),$(shell echo $(GITHUB_SHA) | cut -c1-7),$(shell git rev-parse --short HEAD 2>/dev/null || echo "dev"))
export GIT_COMMIT

# The musl builder image `candidate-native-linux` builds inside
# (make/release-candidate.mk). Versions live beside the Dockerfile, in the
# playbook that owns publishing it, so there is one place a base moves.
BUILDER_DIR := playbooks/operations/ci_rust_images
BUILDER_RUST_VERSION := $(shell sed -n 's/^RUST_VERSION=//p' $(BUILDER_DIR)/versions.env)
BUILDER_ALPINE_SERIES := $(shell sed -n 's/^ALPINE_SERIES=//p' $(BUILDER_DIR)/versions.env)
BUILDER_REVISION := $(shell sed -n 's/^IMAGE_REVISION=//p' $(BUILDER_DIR)/versions.env)
BUILDER_IMAGE ?= ghcr.io/agentsfleet/ci-rust-alpine:$(BUILDER_RUST_VERSION)-alpine$(BUILDER_ALPINE_SERIES)$(if $(BUILDER_REVISION),-$(BUILDER_REVISION))

sync-version: ## Propagate VERSION → rustd/Cargo.toml + cli/package.json (cli.js reads pkg.version at runtime)
	@set -e; \
	V="$$(cat VERSION)"; \
	perl -i -pe 's/"version": "[^"]+"/"version": "'"$$V"'"/;' cli/package.json; \
	perl -i -pe 's/^version = "[^"]+"/version = "'"$$V"'"/;' rustd/Cargo.toml; \
	echo "✓ version $$V synced → cli/package.json, rustd/Cargo.toml (cli.js reads it at runtime)"

check-version: ## Verify cli/package.json and rustd/Cargo.toml match VERSION
	@set -e; \
	V="$$(cat VERSION)"; \
	FAIL=0; \
	grep -q "\"version\": \"$$V\"" cli/package.json \
		|| { printf 'DRIFT  cli/package.json: %s\n' "$$(grep '"version"' cli/package.json | head -1 | xargs)"; FAIL=1; }; \
	grep -q "^version = \"$$V\"" rustd/Cargo.toml \
		|| { printf 'DRIFT  rustd/Cargo.toml [workspace.package]: %s\n' "$$(grep -m1 '^version =' rustd/Cargo.toml | xargs)"; FAIL=1; }; \
	[ "$$FAIL" = "0" ] && echo "✓ all versions match $$V" || { echo "Run: make sync-version"; exit 1; }

# The runner's toolbox: the read-only root every sandbox runs on. Reproducible
# from scripts/toolbox/manifest.txt, so the file name (its SHA-256) names the
# exact root file system; the runner refuses an image whose bytes disagree.
# Linux only, as root (mmdebstrap builds a chroot): `sudo make toolbox-image`.
TOOLBOX_DIR ?= $(CURDIR)/dist/toolbox

toolbox-image:  ## Build the runner's toolbox image (Linux, root; mmdebstrap + erofs-utils) into $(TOOLBOX_DIR)
	@bash scripts/toolbox/build.sh "$(TOOLBOX_DIR)"
