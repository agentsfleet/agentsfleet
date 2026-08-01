#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <check|apply|verify> <dev|prod> <provider>" >&2
  exit 2
fi

action="$1"
environment="$2"
provider="$3"

case "$action" in
  check | apply | verify) ;;
  *)
    echo "ERROR: action must be check, apply, or verify" >&2
    exit 2
    ;;
esac

case "$environment" in
  dev | prod) ;;
  *)
    echo "ERROR: environment must be dev or prod" >&2
    exit 2
    ;;
esac

case "$provider" in
  grafana) ;;
  *)
    echo "ERROR: unsupported observability provider: $provider" >&2
    exit 2
    ;;
esac

case "$action" in
  check) "$SCRIPT_DIR/providers/grafana/check.sh" "$environment" ;;
  apply) "$SCRIPT_DIR/providers/grafana/apply.sh" "$environment" ;;
  verify) "$SCRIPT_DIR/providers/grafana/verify.sh" "$environment" ;;
esac

echo "PASS: $provider observability $action completed for $environment"
