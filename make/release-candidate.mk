# The workflows orchestrate these same local build and promotion commands.
.PHONY: candidate-native-linux candidate-native-macos candidate-image
.PHONY: candidate-identity candidate-create candidate-select candidate-verify
.PHONY: candidate-package candidate-publish-runtime candidate-qualify
.PHONY: candidate-select-development candidate-verify-development candidate-promote-runtime
.PHONY: candidate-verify-signature

candidate-native-linux:
	@CANDIDATE_BUILDER="$(BUILDER_IMAGE)" bash scripts/build-candidate.sh linux

candidate-native-macos:
	@bash scripts/build-candidate.sh macos

candidate-image:
	@bash scripts/build-candidate.sh image

candidate-identity candidate-create candidate-select candidate-verify \
candidate-package candidate-publish-runtime candidate-qualify \
candidate-select-development candidate-verify-development candidate-promote-runtime candidate-verify-signature:
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/candidate.py $(@:candidate-%=%)
