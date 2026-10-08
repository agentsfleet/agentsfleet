#!/usr/bin/env bash
# layout.sh — where the runner deploy puts things on a host, and the files the
# host deploy is made of.
#
# Sourced by deploy.sh on the host and by playbooks/lib/runner/common.sh on the
# machine that drives the deploy, so both name one set of paths. Defines
# readonly constants only, so a shell sources it once. The playbook splices
# these into the commands it runs over Tailscale SSH: every value is a fixed
# string with no quote, space or shell metacharacter in it, except the
# space-separated HOST_STAGING_DIRS list.
# shellcheck disable=SC2034  # every constant is read by the files that source this one

# Everything under here belongs to the deploy user, as host preparation sets it
# up: the playbook copies the binary, the deploy files and the toolbox set in
# without root, and deploy.sh installs each from there as root.
readonly HOST_ROOT="/opt/agentsfleet"
readonly HOST_BIN_DIR="$HOST_ROOT/bin"
readonly HOST_DEPLOY_DIR="$HOST_ROOT/deploy"
readonly HOST_TOOLBOX_DIR="$HOST_ROOT/toolbox"
readonly HOST_STAGING_DIRS="$HOST_BIN_DIR $HOST_DEPLOY_DIR $HOST_TOOLBOX_DIR"
# The runner's environment as the playbook writes it, and the copy deploy.sh
# installs for the unit's EnvironmentFile=.
readonly HOST_ENV_FILE="$HOST_ROOT/.env"
readonly UNIT_ENV_FILE="/etc/default/agentsfleet-runner"

readonly BINARY_NAME="agentsfleet-runner"
readonly INSTALL_DIR="/usr/local/bin"
readonly SERVICE_NAME="agentsfleet-runner.service"
readonly SYSTEMD_DIR="/etc/systemd/system"

# Every file deploy.sh needs beside it on the host, as `<file>:<mode>`: itself,
# the libraries it sources from its own directory, and the unit it installs.
# The playbook copies exactly these, so a file deploy.sh starts sourcing is
# added here or the deploy fails; layout_test.sh sources deploy.sh from a
# directory holding only these to catch that before a host does.
readonly HOST_DEPLOY_FILES=(
  "deploy.sh:755"
  "layout.sh:644"
  "log.sh:644"
  "toolbox.sh:644"
  "service.sh:644"
  "$SERVICE_NAME:644"
)
