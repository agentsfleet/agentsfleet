# The workflows orchestrate these same local build and promotion commands.
.PHONY: candidate-native-linux candidate-native-macos candidate-image
.PHONY: candidate-identity candidate-create candidate-select candidate-verify
.PHONY: candidate-package candidate-publish-runtime candidate-qualify
.PHONY: candidate-select-development candidate-verify-development candidate-promote-runtime
.PHONY: candidate-verify-signature

candidate-native-linux:  ## Build CANDIDATE_PACKAGE for linux/CANDIDATE_ARCH inside BUILDER_IMAGE and prove the binary static (CI)
	@CANDIDATE_BUILDER="$(BUILDER_IMAGE)" bash scripts/build-candidate.sh linux

candidate-native-macos:  ## Build the darwin-arm64 daemon at the dist profile (CI)
	@bash scripts/build-candidate.sh macos

candidate-image:  ## Build the daemon's runtime image from the candidate binary, prove it loads, save it (CI)
	@bash scripts/build-candidate.sh image

# One recipe: each target runs scripts/candidate.py with its name after `candidate-`.
candidate-identity:  ## Print the source tree and VERSION a candidate is keyed by (CI)
candidate-create:  ## Record the built artifacts' source commit and workflow run in the candidate (CI)
candidate-select:  ## Find the workflow run and artifact holding this tree's candidate (CI)
candidate-select-development:  ## Find the run and artifact holding this tree's development-qualified candidate (CI)
candidate-verify:  ## Verify a downloaded candidate against this tree, VERSION, its source and its run (CI)
candidate-package:  ## Package the candidate's binaries into release archives (CI)
candidate-verify-signature:  ## Verify the toolbox signature against the release key the runner pins (CI)
candidate-publish-runtime:  ## Publish the signed candidate's runtime image and print its digest (CI)
candidate-qualify:  ## Record that every development gate passed on the published candidate (CI)
candidate-verify-development:  ## Verify the development receipt, its rollout and its run before promotion (CI)
candidate-promote-runtime:  ## Promote the development-qualified runtime image and print it (CI)
candidate-identity candidate-create candidate-select candidate-verify \
candidate-package candidate-publish-runtime candidate-qualify \
candidate-select-development candidate-verify-development candidate-promote-runtime candidate-verify-signature:
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/candidate.py $(@:candidate-%=%)
