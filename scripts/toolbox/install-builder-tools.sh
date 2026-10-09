#!/usr/bin/env bash
# install-builder-tools.sh — install what scripts/toolbox/build.sh needs on a
# Debian-family Continuous Integration (CI) runner, plus any packages named as
# arguments.
#
# mmdebstrap builds the root in unshare mode, so the runner's own user needs
# subordinate ids (uidmap); the keyring is what every package is checked
# against; erofs-utils writes and dumps the image. Two callers share this list:
# the build-toolbox action, and the kernel lane's coverage job, which also asks
# for bubblewrap.
set -euo pipefail

readonly BUILDER_TOOLS=(mmdebstrap erofs-utils debian-archive-keyring uidmap python3 curl)

sudo apt-get update -qq
sudo apt-get install -y -qq --no-install-recommends "${BUILDER_TOOLS[@]}" "$@"
