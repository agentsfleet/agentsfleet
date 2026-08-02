#!/usr/bin/env bash

# GitHub-hosted runners used by the playbook gate do not include ripgrep.
# These test-only calls use the small common flag subset below.
if [ "${PLAYBOOKS_TEST_FORCE_GREP:-0}" = "1" ] || ! command -v rg >/dev/null 2>&1; then
  rg() {
    local matcher="-E"
    local -a flags=()

    while [ "$#" -gt 0 ]; do
      case "$1" in
        --fixed-strings) matcher="-F" ;;
        --quiet) flags+=("-q") ;;
        -c) flags+=("-c") ;;
        -l) flags+=("-l") ;;
        --)
          shift
          break
          ;;
        *) break ;;
      esac
      shift
    done

    local pattern="$1"
    shift
    local path
    for path in "$@"; do
      [ -d "$path" ] && flags+=("-R") && break
    done

    command grep "$matcher" "${flags[@]}" -- "$pattern" "$@"
  }
fi
