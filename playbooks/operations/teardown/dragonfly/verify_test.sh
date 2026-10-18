#!/usr/bin/env bash
#
# The Dragonfly teardown against a stubbed cluster, with no server.
#
# flyctl's `ssh console --command` runs here in a local shell, and redis-cli
# answers from a fixture: a CLUSTER SHARDS view, a key count per member, and
# members that do not answer. That exercises the parts that decide whether a
# teardown is complete: reading the members from the cluster's own view,
# refusing a partial or unhealthy view before anything is flushed, flushing
# every primary and only primaries, and proving each member empty. The live
# proof against a real four-member cluster is in 001_playbook.md.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly CHECK="$SCRIPT_DIR/01_credential_check.sh"
readonly TEARDOWN="$SCRIPT_DIR/02_teardown.sh"
readonly VERIFY="$SCRIPT_DIR/03_verify.sh"
readonly RESTART_INSTRUCTION="restart or redeploy every agentsfleetd machine"
readonly EMPTY_VERDICT="SUCCESS: every member is empty"

passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf -- "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

# Runs the remote command in a local shell, so cluster_ops.sh meets the
# redis-cli stub below. The app name is recorded to prove the targeting.
cat >"$stub_dir/flyctl" <<'STUB'
#!/usr/bin/env bash
case "$1 ${2:-}" in
  "auth whoami") [ "${STUB_FLY_AUTH:-ok}" = ok ] ;;
  "ssh console")
    shift 2
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --app) printf 'app %s\n' "$2" >>"$CALLS"; shift 2 ;;
        --command) cmd="$2"; shift 2 ;;
        *) shift ;;
      esac
    done
    exec sh -c "$cmd"
    ;;
  *) exit 1 ;;
esac
STUB

# The image's coreutils `timeout`; macOS has none, and nothing here hangs.
cat >"$stub_dir/timeout" <<'STUB'
#!/usr/bin/env bash
shift
exec "$@"
STUB

# Answers `-h <ip> -p <port> <command>` from the fixture.
cat >"$stub_dir/redis-cli" <<'STUB'
#!/usr/bin/env bash
port="$4"
command="$5"
case " ${STUB_DOWN:-} " in *" $port "*) exit 1 ;; esac
case "$command" in
  cluster) cat "$STUB_SHARDS" ;;
  ping) echo PONG ;;
  flushall) printf 'flushall %s\n' "$port" >>"$CALLS"; echo OK ;;
  dbsize)
    var="STUB_KEYS_$port"
    echo "${!var:-0}"
    ;;
  *) exit 1 ;;
esac
STUB
chmod +x "$stub_dir"/*

# One CLUSTER SHARDS view in redis-cli's field/value lines: two shards, each a
# primary and a replica, on IPv6 6PN-style addresses as deployed members announce.
shards_view() {
  local health_of_last="${1:-online}" drop_last="${2:-no}"
  local nodes=("dfly-a fdaa:0:1::2 7001 master" "dfly-a-replica fdaa:0:1::2 7002 replica"
    "dfly-b fdaa:0:1::2 7003 master" "dfly-b-replica fdaa:0:1::2 7004 replica")
  local i id ip port role health
  for i in 0 1 2 3; do
    [ "$i" -eq 3 ] && [ "$drop_last" = yes ] && continue
    read -r id ip port role <<<"${nodes[$i]}"
    health=online
    [ "$i" -eq 3 ] && health="$health_of_last"
    if [ $((i % 2)) -eq 0 ]; then
      printf 'slots\n%s\n%s\nnodes\n' "$((i * 4096))" "$((i * 4096 + 8191))"
    fi
    printf 'id\n%s\nendpoint\n%s\nip\n%s\nport\n%s\nrole\n%s\nreplication-offset\n0\nhealth\n%s\n' \
      "$id" "$ip" "$ip" "$port" "$role" "$health"
  done
}

shards="$work_dir/shards"

# run <script> [extra env assignments...]; stdin is the typed confirmation.
run() {
  local script="$1"
  shift
  : >"$calls"
  env PATH="$stub_dir:$PATH" FLYCTL=flyctl CALLS="$calls" STUB_SHARDS="$shards" \
    DRAGONFLY_REPLICA_POLLS=0 ALLOW_DRAGONFLY_TEARDOWN=1 ENV=dev "$@" bash "$script" 2>&1
}

test_check_reads_every_member_from_the_cluster_view() {
  local name="the preflight lists all four members of the dev app's view"
  local out status=0
  shards_view >"$shards"
  out="$(run "$CHECK")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "status=$status: $out"
  elif ! grep -Fqx "app dragonfly-dev" "$calls"; then
    bad "$name" "the run did not target dragonfly-dev: $(cat "$calls")"
  elif [ "$(grep -c 'fdaa:0:1::2:700[1-4] .* online' <<<"$out")" -ne 4 ]; then
    bad "$name" "four online members were not listed: $out"
  elif grep -q flushall "$calls"; then
    bad "$name" "the read-only preflight flushed: $(cat "$calls")"
  else
    ok "$name"
  fi
}

test_check_refuses_without_approval_or_fly_session() {
  local name="the preflight refuses without approval or a Fly session"
  local out_approval out_auth status_approval=0 status_auth=0
  shards_view >"$shards"
  out_approval="$(run "$CHECK" ALLOW_DRAGONFLY_TEARDOWN=0)" || status_approval=$?
  out_auth="$(run "$CHECK" STUB_FLY_AUTH=no)" || status_auth=$?
  if [ "$status_approval" -eq 0 ]; then
    bad "$name" "ran without ALLOW_DRAGONFLY_TEARDOWN: $out_approval"
  elif [ "$status_auth" -eq 0 ] || [[ "$out_auth" != *"not authenticated"* ]]; then
    bad "$name" "ran without a Fly session: $out_auth"
  else
    ok "$name"
  fi
}

test_teardown_flushes_every_primary_and_only_primaries() {
  local name="the teardown flushes both primaries and no replica"
  local out status=0
  shards_view >"$shards"
  out="$(printf 'DEVELOPMENT\n' | run "$TEARDOWN")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "status=$status: $out"
  elif [ "$(grep -c '^flushall' "$calls")" -ne 2 ] || ! grep -Fqx "flushall 7001" "$calls" ||
    ! grep -Fqx "flushall 7003" "$calls"; then
    bad "$name" "expected FLUSHALL on 7001 and 7003 only: $(cat "$calls")"
  else
    ok "$name"
  fi
}

test_teardown_flushes_nothing_without_the_typed_name() {
  local name="a wrong or missing confirmation flushes nothing"
  local out_wrong out_missing status_wrong=0 status_missing=0
  shards_view >"$shards"
  out_wrong="$(printf 'dev\n' | run "$TEARDOWN")" || status_wrong=$?
  local wrong_calls
  wrong_calls="$(grep -c '^flushall' "$calls")"
  out_missing="$(run "$TEARDOWN" </dev/null)" || status_missing=$?
  if [ "$status_wrong" -eq 0 ] || [ "$wrong_calls" -ne 0 ]; then
    bad "$name" "typed 'dev' and it ran ($wrong_calls flush): $out_wrong"
  elif [ "$status_missing" -eq 0 ] || grep -q '^flushall' "$calls"; then
    bad "$name" "no confirmation and it ran: $out_missing"
  else
    ok "$name"
  fi
}

test_teardown_refuses_a_partial_view_before_flushing() {
  local name="a view naming three of four members is refused before any flush"
  local out status=0
  shards_view online yes >"$shards"
  out="$(printf 'DEVELOPMENT\n' | run "$TEARDOWN")" || status=$?
  if [ "$status" -eq 0 ] || grep -q '^flushall' "$calls"; then
    bad "$name" "a partial cluster was flushed: $(cat "$calls") $out"
  elif [[ "$out" != *"names 3 member(s), not 4"* ]]; then
    bad "$name" "the refusal does not name the count: $out"
  else
    ok "$name"
  fi
}

test_teardown_refuses_a_silent_member_before_flushing() {
  local name="a member that does not answer stops the teardown before any flush"
  local out status=0
  shards_view >"$shards"
  out="$(printf 'DEVELOPMENT\n' | run "$TEARDOWN" STUB_DOWN=7004)" || status=$?
  if [ "$status" -eq 0 ] || grep -q '^flushall' "$calls"; then
    bad "$name" "flushed with 7004 silent: $(cat "$calls") $out"
  elif [[ "$out" != *"fdaa:0:1::2:7004 (replica) did not answer PING"* ]]; then
    bad "$name" "the silent member is not named: $out"
  else
    ok "$name"
  fi
}

test_verify_passes_an_empty_cluster_and_asks_for_a_restart() {
  local name="verify passes when every member reports 0 and asks for a restart"
  local out status=0
  shards_view >"$shards"
  out="$(run "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "status=$status: $out"
  elif [ "$(grep -c 'fdaa:0:1::2:700[1-4] .* keys=0' <<<"$out")" -ne 4 ]; then
    bad "$name" "not every member's count was printed: $out"
  elif [[ "$out" != *"$EMPTY_VERDICT"* ]] || [[ "$out" != *"$RESTART_INSTRUCTION"* ]]; then
    bad "$name" "missing verdict or restart instruction: $out"
  else
    ok "$name"
  fi
}

test_verify_fails_on_any_unproven_member() {
  local name="verify fails on keys left on a replica, an offline member, or a silent one"
  local case_name expected out status
  shards_view >"$shards"
  for case_name in keys offline silent; do
    status=0
    case "$case_name" in
      keys)
        expected="2 member(s) still hold keys"
        out="$(run "$VERIFY" STUB_KEYS_7003=9 STUB_KEYS_7004=9)" || status=$?
        ;;
      offline)
        shards_view fail >"$shards"
        expected="fdaa:0:1::2:7004 (replica) is fail, not online"
        out="$(run "$VERIFY")" || status=$?
        shards_view >"$shards"
        ;;
      silent)
        expected="no answer within 5s"
        out="$(run "$VERIFY" STUB_DOWN=7002)" || status=$?
        ;;
    esac
    if [ "$status" -eq 0 ] || [[ "$out" == *"$EMPTY_VERDICT"* ]]; then
      bad "$name" "$case_name: an unproven cluster passed: $out"
      return
    elif [[ "$out" != *"$expected"* ]]; then
      bad "$name" "$case_name: expected '$expected' in: $out"
      return
    fi
  done
  ok "$name"
}

test_prod_targets_the_prod_app() {
  local name="ENV=prod targets dragonfly-prod and nothing else"
  local out status=0
  shards_view >"$shards"
  out="$(run "$VERIFY" ENV=prod)" || status=$?
  if [ "$status" -ne 0 ] || ! grep -Fqx "app dragonfly-prod" "$calls" || grep -q dragonfly-dev "$calls"; then
    bad "$name" "status=$status calls=$(cat "$calls"): $out"
  else
    ok "$name"
  fi
}

test_check_reads_every_member_from_the_cluster_view
test_check_refuses_without_approval_or_fly_session
test_teardown_flushes_every_primary_and_only_primaries
test_teardown_flushes_nothing_without_the_typed_name
test_teardown_refuses_a_partial_view_before_flushing
test_teardown_refuses_a_silent_member_before_flushing
test_verify_passes_an_empty_cluster_and_asks_for_a_restart
test_verify_fails_on_any_unproven_member
test_prod_targets_the_prod_app

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
