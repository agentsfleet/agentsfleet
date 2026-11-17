# =============================================================================
# agentsfleet MAKEFILE - MODULAR STRUCTURE
# =============================================================================

# Wraps a long-running command with a quiet-triggered heartbeat, so a lane that
# spends minutes inside one silent `cargo` invocation can be told apart from one
# that has wedged. Progress goes to stderr, so a lane that pipes its command
# through `tee` into a parsed tally is unaffected. Set WITH_PROGRESS_DISABLE=1
# to pass commands straight through.
WITH_PROGRESS := $(CURDIR)/scripts/with-progress.sh

include make/dev.mk
include make/quality.mk
include make/check-safety-gates.mk
include make/test.mk
include make/build.mk
include make/release-candidate.mk
.DEFAULT_GOAL := help

# Generated from the `## ` text on each rule line, so a new target documents
# itself where it is declared. `_`-prefixed targets are internal and stay out.
help:  ## List every public target with its description, grouped by the file that declares it
	@echo "agentsfleet — make <target>"
	@awk 'BEGIN { FS = ":[^#]*## " } \
	  /^[A-Za-z0-9][A-Za-z0-9_.-]*:([^=]|$$)/ && /## / { \
	    if (FILENAME != shown) { printf "\n%s\n", FILENAME; shown = FILENAME } \
	    printf "  %-34s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
