#!/bin/sh
# cluster_ops.sh - list, flush or verify every member of a Dragonfly cluster.
#
#     cluster_ops.sh members|flush|verify <expected-members>
#
# Runs where every announced member address answers: inside the Dragonfly
# app's own machine. 02_teardown.sh and 03_verify.sh ship it there over
# `flyctl ssh console` (fly_exec.sh), because the members announce private
# 6PN addresses and the app has no public service. POSIX sh, because the
# image's /bin/sh is dash.
#
# The cluster's own view (CLUSTER SHARDS at the seed) names the members. A view
# holding fewer members than expected, a member that is not online, or a
# member that does not answer fails the run, so a partial flush can never
# read as a complete one.
set -eu

SEED_HOST="${DRAGONFLY_SEED_HOST:-127.0.0.1}"
SEED_PORT="${DRAGONFLY_SEED_PORT:-7001}"
# The password scripts/dragonfly-cluster.sh starts every node with. redis-cli
# reads it from the environment, so it never appears in a process listing.
REDISCLI_AUTH="${DRAGONFLY_PASSWORD:-agentsfleet}"
export REDISCLI_AUTH
# A replica empties when its primary's FLUSHALL replicates; this bounds the
# wait for that, about ten seconds.
REPLICA_POLLS="${DRAGONFLY_REPLICA_POLLS:-20}"
REPLICA_POLL_SECONDS=0.5
# A member that accepts the connection and never answers (a frozen process)
# must fail the run rather than hang it.
MEMBER_TIMEOUT_SECONDS=5

fail() {
	printf '✗ %s\n' "$1" >&2
	exit 1
}

cli() {
	host=$1
	port=$2
	shift 2
	timeout "$MEMBER_TIMEOUT_SECONDS" redis-cli -h "$host" -p "$port" "$@"
}

# What a failed call printed, or the timeout when it printed nothing.
no_answer() {
	if [ -n "$1" ]; then
		printf '%s\n' "$1"
	else
		printf 'no answer within %ss\n' "$MEMBER_TIMEOUT_SECONDS"
	fi
}

# One line per member: `<ip> <port> <role> <health>`, read from the field and
# value lines CLUSTER SHARDS prints.
members() {
	shards=$(cli "$SEED_HOST" "$SEED_PORT" cluster shards 2>&1) ||
		fail "the seed $SEED_HOST:$SEED_PORT did not answer CLUSTER SHARDS: $shards"
	printf '%s\n' "$shards" | awk '
		prev == "ip" { ip = $0 }
		prev == "port" { port = $0 }
		prev == "role" { role = $0 }
		prev == "health" { print ip, port, role, $0 }
		{ prev = $0 }'
}

# The member list, refused unless it holds `expected` members, all online.
checked_members() {
	expected=$1
	list=$(members)
	count=$(printf '%s\n' "$list" | grep -c . || true)
	printf 'cluster view from %s:%s: %s member(s), %s expected\n' \
		"$SEED_HOST" "$SEED_PORT" "$count" "$expected" >&2
	[ "$count" -eq "$expected" ] ||
		fail "the cluster names $count member(s), not $expected; refusing to act on part of it"
	while read -r ip port role health; do
		[ "$health" = "online" ] || fail "member $ip:$port ($role) is $health, not online"
		# The view says online from its configuration; only an answer proves the
		# process is serving. Checked before any verb acts, so a flush never
		# starts against a cluster it cannot then verify.
		answer=$(cli "$ip" "$port" ping 2>&1) || answer=$(no_answer "$answer")
		[ "$answer" = "PONG" ] || fail "member $ip:$port ($role) did not answer PING: $answer"
	done <<EOF
$list
EOF
	printf '%s\n' "$list"
}

list_members() {
	# Captured first: a refusal inside a pipeline's left side would be lost.
	list=$(checked_members "$1")
	while read -r ip port role health; do
		printf '  %s:%s %s %s\n' "$ip" "$port" "$role" "$health"
	done <<EOF
$list
EOF
}

flush() {
	list=$(checked_members "$1")
	flushed=0
	while read -r ip port role _health; do
		[ "$role" = "master" ] || continue
		answer=$(cli "$ip" "$port" flushall 2>&1) || answer=$(no_answer "$answer")
		[ "$answer" = "OK" ] || fail "primary $ip:$port refused FLUSHALL: $answer"
		printf '  flushed primary %s:%s\n' "$ip" "$port"
		flushed=$((flushed + 1))
	done <<EOF
$list
EOF
	[ "$flushed" -gt 0 ] || fail "the cluster view names no primary"
	printf '✓ FLUSHALL sent to %s primary member(s)\n' "$flushed"
}

# A member's key count, waiting for a replica to catch up with its primary.
member_keys() {
	ip=$1
	port=$2
	role=$3
	polls=0
	while :; do
		keys=$(cli "$ip" "$port" dbsize 2>&1) || fail "member $ip:$port ($role) did not answer DBSIZE: $(no_answer "$keys")"
		case "$keys" in
			'' | *[!0-9]*) fail "member $ip:$port ($role) answered DBSIZE with: $keys" ;;
		esac
		if [ "$keys" -eq 0 ] || [ "$role" = "master" ] || [ "$polls" -ge "$REPLICA_POLLS" ]; then
			printf '%s\n' "$keys"
			return 0
		fi
		polls=$((polls + 1))
		sleep "$REPLICA_POLL_SECONDS"
	done
}

verify() {
	list=$(checked_members "$1")
	holding=0
	while read -r ip port role _health; do
		keys=$(member_keys "$ip" "$port" "$role")
		printf '  %s:%s %s keys=%s\n' "$ip" "$port" "$role" "$keys"
		[ "$keys" -eq 0 ] || holding=$((holding + 1))
	done <<EOF
$list
EOF
	[ "$holding" -eq 0 ] || fail "$holding member(s) still hold keys"
	printf '✓ every member reports DBSIZE 0\n'
}

main() {
	[ "$#" -eq 2 ] || fail "usage: cluster_ops.sh members|flush|verify <expected-members>"
	case "$2" in
		'' | *[!0-9]*) fail "expected-members must be a number, got: $2" ;;
	esac
	case "$1" in
		members) list_members "$2" ;;
		flush) flush "$2" ;;
		verify) verify "$2" ;;
		*) fail "unknown verb: $1" ;;
	esac
}

main "$@"
