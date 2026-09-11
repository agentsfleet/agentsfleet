#!/usr/bin/env bash
# The region move refuses the wrong shape and never lets a password reach argv.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly GATE="$SCRIPT_DIR/00_gate.sh"
readonly CHECK="$SCRIPT_DIR/01_credential_check.sh"
readonly COPY="$SCRIPT_DIR/02_copy.sh"
readonly VERIFY="$SCRIPT_DIR/03_verify.sh"
readonly SECRET="do-not-print-provider-secret"
readonly LIVE_HOST="aws-us-east-2-2.pg.psdb.cloud"

passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

# Live strings on the Ohio host; `next-*` on NEXT_HOST, the api one on
# NEXT_API_PORT. Those two and the two target answers below are the only
# things a test varies.
cat >"$stub_dir/op" <<STUB
#!/usr/bin/env bash
case "\${1:-}" in
  whoami) printf 'stub-user\\n' ;;
  read)
    case "\${2:-}" in
      */migrator-connection-string) printf 'postgres://m:$SECRET@$LIVE_HOST:5432/db\\n' ;;
      */next-migrator-connection-string) printf 'postgres://m:$SECRET@%s:5432/db\\n' "\$NEXT_HOST" ;;
      */next-api-connection-string) printf 'postgres://a:$SECRET@%s:%s/db\\n' "\$NEXT_HOST" "\${NEXT_API_PORT:-6432}" ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

# Answers the three questions the scripts put to a container: the ledger
# version (branching on which URL was forwarded), a census, or a copy.
cat >"$stub_dir/docker" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
case "$*" in
  *schema_migrations*)
    case "${DATABASE_URL:-}" in
      *"$NEXT_HOST"*) printf '%s\n' "${TARGET_LEDGER:-900}" ;;
      *) printf '900\n' ;;
    esac
    ;;
  *census.sql*)
    case "${DATABASE_URL:-}" in
      *"$NEXT_HOST"*) printf '%b' "${TARGET_CENSUS:-core.fleets=3\nledger.version=900\n}" ;;
      *) printf 'core.fleets=3\nledger.version=900\n' ;;
    esac
    ;;
  *) exit 0 ;;
esac
STUB
chmod +x "$stub_dir/op" "$stub_dir/docker"

run() {
  local script="$1"
  shift
  : >"$calls"
  env PATH="$stub_dir:$PATH" CALLS="$calls" NEXT_HOST="${NEXT_HOST:-us-east.pg.psdb.cloud}" \
    ALLOW_VAULT_READS=1 ENV="${ENV_UNDER_TEST:-dev}" "$@" bash "$script" 2>&1
}

test_gate_refuses_all() {
  local out status=0
  out="$(ENV_UNDER_TEST=all run "$GATE")" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"ENV must be 'dev' or 'prod'"* ]]; then
    ok "gate refuses ENV=all"
  else
    bad "gate refuses ENV=all" "status=$status: $out"
  fi
}

test_check_refuses_same_host() {
  local out status=0
  out="$(NEXT_HOST="$LIVE_HOST" run "$CHECK")" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"name the live host"* ]]; then
    ok "check refuses next-* strings on the live host"
  else
    bad "check refuses next-* strings on the live host" "status=$status: $out"
  fi
}

test_check_refuses_api_on_direct_port() {
  local out status=0
  out="$(run "$CHECK" NEXT_API_PORT=5432)" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"next api (PgBouncer) must name port 6432 (found: 5432)"* ]]; then
    ok "check refuses a next api string on 5432"
  else
    bad "check refuses a next api string on 5432" "status=$status: $out"
  fi
}

test_check_passes_and_names_hosts_only() {
  local out status=0
  out="$(run "$CHECK")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "check passes a staged branch" "status=$status: $out"
  elif [[ "$out" == *"$SECRET"* ]]; then
    bad "check passes a staged branch" "a password reached the output"
  elif [[ "$out" != *"source host: $LIVE_HOST"* ]] ||
       [[ "$out" != *"target host: us-east.pg.psdb.cloud"* ]]; then
    bad "check passes a staged branch" "hosts not reported: $out"
  else
    ok "check passes a staged branch and prints hosts, not strings"
  fi
}

test_copy_refuses_without_approval() {
  local out status=0
  out="$(run "$COPY" </dev/null)" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"ALLOW_PROVIDER_WRITES=1 required"* ]]; then
    ok "copy refuses without ALLOW_PROVIDER_WRITES"
  else
    bad "copy refuses without ALLOW_PROVIDER_WRITES" "status=$status: $out"
  fi
}

test_copy_refuses_an_unmigrated_target() {
  local out status=0
  out="$(run "$COPY" ALLOW_PROVIDER_WRITES=1 TARGET_LEDGER=0 </dev/null)" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"not migrated to the source's version"* ]]; then
    ok "copy refuses a target whose ledger is behind"
  else
    bad "copy refuses a target whose ledger is behind" "status=$status: $out"
  fi
}

test_copy_keeps_passwords_out_of_argv() {
  local out status=0
  out="$(printf 'dev\n' | run "$COPY" ALLOW_PROVIDER_WRITES=1)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "copy forwards strings by name" "status=$status: $out"
  elif grep -q "$SECRET" "$calls"; then
    bad "copy forwards strings by name" "a password reached docker's argv"
  elif ! grep -q -- '--exclude-table=audit.schema_migrations' "$calls"; then
    bad "copy forwards strings by name" "the ledger was not excluded: $(cat "$calls")"
  else
    ok "copy forwards strings by name and excludes the ledger"
  fi
}

test_copy_refuses_a_wrong_confirmation() {
  local out status=0
  out="$(printf 'prod\n' | run "$COPY" ALLOW_PROVIDER_WRITES=1)" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"Confirmation failed"* ]] && ! grep -q pg_dump "$calls"; then
    ok "copy stops on a wrong typed confirmation"
  else
    bad "copy stops on a wrong typed confirmation" "status=$status: $out"
  fi
}

test_verify_fails_on_a_row_count_gap() {
  local out status=0
  out="$(run "$VERIFY" TARGET_CENSUS='core.fleets=2\nledger.version=900\n')" || status=$?
  if [ "$status" -ne 0 ] && [[ "$out" == *"the branches differ"* ]] && [[ "$out" == *"-core.fleets=3"* ]]; then
    ok "verify fails when a table's rows differ"
  else
    bad "verify fails when a table's rows differ" "status=$status: $out"
  fi
}

test_verify_passes_matching_branches() {
  local out status=0
  out="$(run "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ] && [[ "$out" == *"every table and the ledger match"* ]]; then
    ok "verify passes matching branches"
  else
    bad "verify passes matching branches" "status=$status: $out"
  fi
}

# The container's psql has no trust store, and the daemon rejects the flag
# that fixes it. Both halves of that are load-bearing, so both are pinned.
test_system_roots_are_added_for_the_container() {
  local name="system roots are appended for the container, once"
  # shellcheck source=./lib.sh
  ( set +u; . "$SCRIPT_DIR/lib.sh"
    with_query="$(region_move_with_system_roots 'postgres://u:p@h:5432/db?sslmode=verify-full')"
    without_query="$(region_move_with_system_roots 'postgres://u:p@h:5432/db')"
    already="$(region_move_with_system_roots 'postgres://u:p@h:5432/db?sslrootcert=/a.pem')"
    [ "$with_query" = 'postgres://u:p@h:5432/db?sslmode=verify-full&sslrootcert=system' ] || exit 1
    [ "$without_query" = 'postgres://u:p@h:5432/db?sslrootcert=system' ] || exit 2
    [ "$already" = 'postgres://u:p@h:5432/db?sslrootcert=/a.pem' ] || exit 3
    # The host and port parsers still read a wrapped URL, since the census
    # prints the host it is about to compare.
    [ "$(region_move_url_host "$with_query")" = 'h' ] || exit 4
    [ "$(region_move_url_port "$with_query")" = '5432' ] || exit 5
  ) && ok "$name" || bad "$name" "wrapping or parsing is wrong (exit $?)"
}

# A vault field carrying sslrootcert is a daemon that will not boot:
# afd_db parses it as a path and raises TlsCertFileUnreadable.
test_the_staged_vault_strings_carry_no_sslrootcert() {
  local name="staged vault strings never carry sslrootcert"
  if grep -q 'sslrootcert' "$SCRIPT_DIR/001_playbook.md" &&
     ! grep -q 'never reach a vault field\|must never appear in a vault' "$SCRIPT_DIR/001_playbook.md"; then
    bad "$name" "the playbook mentions sslrootcert without warning it off vault fields"
    return
  fi
  if grep -nE 'next-(api|migrator)-connection-string.*sslrootcert' "$SCRIPT_DIR"/*.sh >/dev/null 2>&1; then
    bad "$name" "a script writes sslrootcert into a staged vault field"
  else
    ok "$name"
  fi
}

# The target is migrated before the copy, so it already holds what the
# migrations wrote. A restore that ignores that collides on migration 410's
# singleton row and substitutes trigger defaults for migration 880's counters.
test_the_load_clears_the_target_and_suppresses_triggers() {
  local name="the load truncates, suppresses triggers, and is atomic"
  local out status=0
  out="$(printf 'dev\n' | run "$COPY" ALLOW_PROVIDER_WRITES=1)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "status=$status: $out"
    return
  fi
  if ! grep -q -- '--single-transaction' "$calls"; then
    bad "$name" "the load is not atomic, so a failure leaves the target half-populated"
  elif ! grep -q -- '--format=plain' "$calls"; then
    bad "$name" "custom format cannot carry a session setting through pg_restore"
  else
    ok "$name"
  fi
}

# The two statements the load depends on live in the generated SQL, not in the
# docker argv, so they are asserted against the file the script writes.
test_the_load_sql_carries_the_two_statements_it_depends_on() {
  local name="the load SQL truncates and sets replica role"
  local body
  body="$(sed -n "/cat >\"\$work_dir\/load.sql\"/,/^SQL$/p" "$SCRIPT_DIR/02_copy.sh")"
  if [ -z "$body" ]; then
    bad "$name" "no load.sql heredoc found in 02_copy.sh"
  elif [[ "$body" != *"session_replication_role = 'replica'"* ]]; then
    bad "$name" "triggers are not suppressed: counters would take trigger defaults"
  elif [[ "$body" != *"TRUNCATE TABLE"* ]] || [[ "$body" != *"CASCADE"* ]]; then
    bad "$name" "the target is not cleared, so seeded rows collide"
  elif [[ "$body" != *"information_schema.tables"* ]]; then
    bad "$name" "the table list is hand-kept and will rot"
  elif [[ "$body" != *"schema_migration%"* ]]; then
    bad "$name" "the ledger is not excluded from the truncate"
  else
    ok "$name"
  fi
}

echo "database-region-move regression tests"
test_the_load_clears_the_target_and_suppresses_triggers
test_the_load_sql_carries_the_two_statements_it_depends_on
test_system_roots_are_added_for_the_container
test_the_staged_vault_strings_carry_no_sslrootcert
test_gate_refuses_all
test_check_refuses_same_host
test_check_refuses_api_on_direct_port
test_check_passes_and_names_hosts_only
test_copy_refuses_without_approval
test_copy_refuses_an_unmigrated_target
test_copy_keeps_passwords_out_of_argv
test_copy_refuses_a_wrong_confirmation
test_verify_fails_on_a_row_count_gap
test_verify_passes_matching_branches

echo ""
echo "results: $passed passed, $failed failed"
test "$failed" -eq 0
