#!/usr/bin/env bash
set -euo pipefail

exec ./playbooks/gates/check-credentials.sh "$@"
