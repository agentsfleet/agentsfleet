#!/usr/bin/env bash
# stop_writers.sh - Dispatched precondition: no writer may be running.
#
# Thin by design. The implementation is shared with the other teardown playbook
# and lives in playbooks/lib/stop_writers.sh; this file exists so the gate
# dispatches a step from its OWN directory, which is what makes the ordering
# testable (see operations/explicit_dispatch_test.sh).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${SCRIPT_DIR}/../../../lib/stop_writers.sh"
