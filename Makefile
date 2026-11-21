
# Prints a heartbeat while a long command is silent, on stderr so a `tee`'d
# tally is unaffected. WITH_PROGRESS_DISABLE=1 passes commands straight through.
WITH_PROGRESS := $(CURDIR)/scripts/with-progress.sh

include make/dev.mk
include make/quality.mk
include make/check-safety-gates.mk
include make/test.mk
include make/build.mk
include make/release-candidate.mk
.DEFAULT_GOAL := help

# Built from each rule's `## ` text; `_`-prefixed targets stay out.
help:  ## List every public target with its description, grouped by the file that declares it
	@echo "agentsfleet — make <target>"
	@awk 'BEGIN { FS = ":[^#]*## " } \
	  /^[A-Za-z0-9][A-Za-z0-9_.-]*:([^=]|$$)/ && /## / { \
	    if (FILENAME != shown) { printf "\n%s\n", FILENAME; shown = FILENAME } \
	    printf "  %-34s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
