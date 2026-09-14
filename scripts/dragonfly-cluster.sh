#!/usr/bin/env bash
# dragonfly-cluster.sh — a real four-node Dragonfly cluster in one container,
# plus one TLS node beside it.
#
#     serve                          entrypoint: start the nodes, bootstrap, wait
#     healthy                        compose healthcheck: every node online
#     migrate <from> <to> <lo> <hi>  move a slot range between primaries
#     reset                          flush every node, restore the layout
#
# The fifth process is not a cluster member. It runs `--cluster_mode=emulated`
# (one node answering as a whole cluster) over TLS, and exists for exactly one
# suite: the trust proof, which asserts the lane's own authority verifies AND
# a foreign one is refused. Every other suite takes the plaintext cluster,
# because a TLS handshake against an RSA-2048 leaf costs ~230 ms and a lane
# opens hundreds of connections — that cost, queued in front of a connect
# budget, is what turns a healthy datastore into ConnectTimeout. The
# certificates are minted once into the data volume: a CA and a LEAF (a trust
# anchor must carry CA:TRUE, an end-entity must not — one self-signed file
# cannot be both, and rustls refuses the shortcut), plus a second, well-formed
# authority that signs nothing here, for the refusal half.
#
# Two primaries and one replica each, all in this container's network
# namespace, every node announcing 127.0.0.1 and the port it listens on. The
# compose service publishes those same port numbers on the host, so a MOVED
# reply names an address that resolves from inside the container, from the host
# and from CI alike. That is the property a multi-container layout cannot give:
# a node advertising a bridge address is unreachable from macOS, and one
# advertising 127.0.0.1 from its own container is unreachable from its peers.
#
# Dragonfly forgets its cluster configuration on restart and expects the
# operator to push it again (the bootstrap sequence in Dragonfly's
# cluster-mode.md), so `serve` bootstraps on
# every start. Slot ownership lives in `$DRAGONFLY_DATA/slots` because a
# migration moves it, and the next start must push the layout the tests left
# rather than the canonical one — `reset` is what puts it back.
#
# The admin ports are bound to loopback and take no password, because a node
# migrating slots dials its peer's admin port and does not authenticate. They
# are therefore never published: every administrative verb runs inside the
# container, through `docker compose exec`, which is also what keeps this
# script unable to address a shared or remote datastore — the ownership
# guarantee the reset path relies on.
set -euo pipefail

BASE_PORT="${DRAGONFLY_BASE_PORT:-7001}"
# Data ports are BASE..BASE+4 (four cluster nodes, then the TLS node); admin
# ports sit beyond them.
ADMIN_OFFSET="${DRAGONFLY_ADMIN_OFFSET:-5}"
PASSWORD="${DRAGONFLY_PASSWORD:-agentsfleet}"
DATA="${DRAGONFLY_DATA:-/data}"
HOST=127.0.0.1
NODE_COUNT=4
# The TLS node's index: one past the cluster, so its ports follow theirs.
TLS_NODE=4
TLS_DIR="$DATA/tls"
# --masterauth is not decoration. Every node sets --requirepass, so a replica
# dialling its primary's DATA port must authenticate like any other client; the
# REPLICAOF below was answered "-NOAUTH Authentication required.", replication
# was cancelled, and both replicas stayed masters holding nothing. The topology
# read as two primaries and two replicas in CLUSTER config while INFO reported
# connected_slaves:0 on both primaries — a declared replica set is not a
# replicating one, and only the second is worth anything to recovery.
#
# Node i is a primary when i is even; node i+1 is its replica. The ids are
# stable across restarts because they are named here, not minted — a cluster
# config names nodes by id, and a re-minted id would orphan every replica.
NODE_IDS=(dfly-a dfly-a-replica dfly-b dfly-b-replica)
PRIMARIES=(0 2)
# The layout every start converges to unless a migration moved it: two
# halves of the 16384 slots, one per primary.
CANONICAL_SLOTS="0 0 8191
2 8192 16383"
SLOTS_FILE="$DATA/slots"
MIGRATION_POLL_SECONDS=0.2
MIGRATION_POLL_LIMIT=300

data_port() { echo $((BASE_PORT + $1)); }
admin_port() { echo $((BASE_PORT + $1 + ADMIN_OFFSET)); }
cli() { local port=$1; shift; redis-cli -h "$HOST" -p "$port" -a "$PASSWORD" --no-auth-warning "$@"; }
tls_cli() { redis-cli -h "$HOST" -p "$(data_port "$TLS_NODE")" --tls --cacert "$TLS_DIR/ca.crt" -a "$PASSWORD" --no-auth-warning "$@"; }
# The admin port answers without a password (see the header), so no `-a`.
admin() { local port=$1; shift; redis-cli -h "$HOST" -p "$port" "$@"; }
replica_of() { echo $(( $1 + 1 )); }

start_nodes() {
  local i
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    mkdir -p "$DATA/n$i"
    # 256 MiB per proactor thread is a hard floor Dragonfly enforces at boot;
    # two threads at 512 MiB is the smallest real (multi-shard) node.
    dragonfly --logtostderr --version_check=false \
      --cluster_mode=yes --cluster_node_id="${NODE_IDS[$i]}" \
      --port="$(data_port "$i")" --admin_port="$(admin_port "$i")" \
      --admin_bind="$HOST" --admin_nopass \
      --cluster_announce_ip="$HOST" --announce_port="$(data_port "$i")" \
      --requirepass="$PASSWORD" --masterauth="$PASSWORD" --dir="$DATA/n$i" \
      --maxmemory=512mb --proactor_threads=2 --lock_on_hashtags \
      >/dev/null 2>"$DATA/n$i/dragonfly.log" &
  done
}

# Mints the trust material once. Certificates are public and readable by
# any uid (the daemon image runs as 65532); keys stay 0600.
mint_certificates() {
  mkdir -p "$TLS_DIR"
  if [ ! -s "$TLS_DIR/ca.crt" ] || [ ! -s "$TLS_DIR/server.crt" ] || [ ! -s "$TLS_DIR/server.key" ]; then
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TLS_DIR/ca.key" -out "$TLS_DIR/ca.crt" -days 3650       -subj "/CN=agentsfleet-local-ca" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
    openssl req -newkey rsa:2048 -nodes -keyout "$TLS_DIR/server.key" -out "$TLS_DIR/server.csr" -subj "/CN=localhost" 2>/dev/null
    printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\n' >"$TLS_DIR/server.ext"
    openssl x509 -req -in "$TLS_DIR/server.csr" -CA "$TLS_DIR/ca.crt" -CAkey "$TLS_DIR/ca.key" -CAcreateserial       -out "$TLS_DIR/server.crt" -days 3650 -extfile "$TLS_DIR/server.ext" 2>/dev/null
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TLS_DIR/foreign-ca.key" -out "$TLS_DIR/foreign-ca.crt" -days 3650       -subj "/CN=agentsfleet-foreign-ca" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
  fi
  chmod 0644 "$TLS_DIR/ca.crt" "$TLS_DIR/server.crt" "$TLS_DIR/foreign-ca.crt"
}

start_tls_node() {
  mint_certificates
  mkdir -p "$DATA/n$TLS_NODE"
  # One proactor thread at the 256 MiB floor: this node serves one suite.
  # The admin port stays plaintext (loopback only) so the readiness and
  # reset paths need no certificate to reach it.
  dragonfly --logtostderr --version_check=false \
    --cluster_mode=emulated \
    --tls --tls_cert_file="$TLS_DIR/server.crt" --tls_key_file="$TLS_DIR/server.key" \
    --port="$(data_port "$TLS_NODE")" --admin_port="$(admin_port "$TLS_NODE")" \
    --admin_bind="$HOST" --admin_nopass --no_tls_on_admin_port \
    --requirepass="$PASSWORD" --dir="$DATA/n$TLS_NODE" \
    --maxmemory=256mb --proactor_threads=1 \
    >/dev/null 2>"$DATA/n$TLS_NODE/dragonfly.log" &
}

wait_for_nodes() {
  local i
  for i in $(seq 0 "$TLS_NODE"); do
    until [ "$(admin "$(admin_port "$i")" ping 2>/dev/null)" = "PONG" ]; do sleep 0.2; done
  done
}

# The DFLYCLUSTER CONFIG document for the layout in $SLOTS_FILE, with an
# optional migration entry appended to one primary's shard.
render_config() {
  local mig_from="${1:-}" mig_to="${2:-}" mig_lo="${3:-}" mig_hi="${4:-}"
  local out="[" first_shard=1 p
  for p in "${PRIMARIES[@]}"; do
    local ranges="" owner lo hi
    while read -r owner lo hi; do
      [ "$owner" = "$p" ] || continue
      ranges="$ranges${ranges:+,}{\"start\":$lo,\"end\":$hi}"
    done <"$SLOTS_FILE"
    [ -n "$ranges" ] || continue
    [ $first_shard -eq 1 ] || out="$out,"
    first_shard=0
    local r
    r=$(replica_of "$p")
    out="$out{\"slot_ranges\":[$ranges],\"master\":{\"id\":\"${NODE_IDS[$p]}\",\"ip\":\"$HOST\",\"port\":$(data_port "$p")},\"replicas\":[{\"id\":\"${NODE_IDS[$r]}\",\"ip\":\"$HOST\",\"port\":$(data_port "$r")}]"
    if [ "$p" = "$mig_from" ]; then
      out="$out,\"migrations\":[{\"node_id\":\"${NODE_IDS[$mig_to]}\",\"ip\":\"$HOST\",\"port\":$(admin_port "$mig_to"),\"slot_ranges\":[{\"start\":$mig_lo,\"end\":$mig_hi}]}]"
    fi
    out="$out}"
  done
  echo "$out]"
}

push_config() {
  local config=$1 i
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    admin "$(admin_port "$i")" dflycluster config "$config" >/dev/null
  done
}

bootstrap() {
  [ -s "$SLOTS_FILE" ] || printf '%s\n' "$CANONICAL_SLOTS" >"$SLOTS_FILE"
  local p
  for p in "${PRIMARIES[@]}"; do
    admin "$(admin_port "$(replica_of "$p")")" replicaof "$HOST" "$(data_port "$p")" >/dev/null
  done
  push_config "$(render_config)"
}

# Rewrites $SLOTS_FILE with [lo,hi] moved from one primary to another. A range
# that only partly overlaps the moved one is split around it.
reassign_slots() {
  local from=$1 to=$2 lo=$3 hi=$4 owner rlo rhi next=""
  while read -r owner rlo rhi; do
    if [ "$owner" = "$from" ] && [ "$rlo" -le "$lo" ] && [ "$rhi" -ge "$hi" ]; then
      [ "$rlo" -lt "$lo" ] && next="$next$from $rlo $((lo - 1))"$'\n'
      [ "$rhi" -gt "$hi" ] && next="$next$from $((hi + 1)) $rhi"$'\n'
    else
      next="$next$owner $rlo $rhi"$'\n'
    fi
  done <"$SLOTS_FILE"
  printf '%s%s %s %s\n' "$next" "$to" "$lo" "$hi" >"$SLOTS_FILE"
}

owns_range() {
  local owner=$1 lo=$2 hi=$3 o rlo rhi
  while read -r o rlo rhi; do
    [ "$o" = "$owner" ] && [ "$rlo" -le "$lo" ] && [ "$rhi" -ge "$hi" ] && return 0
  done <"$SLOTS_FILE"
  return 1
}

migrate() {
  local from=$1 to=$2 lo=$3 hi=$4
  owns_range "$from" "$lo" "$hi" || { echo "✗ node $from does not own $lo-$hi" >&2; exit 1; }
  push_config "$(render_config "$from" "$to" "$lo" "$hi")"
  local n=0
  until admin "$(admin_port "$from")" dflycluster slot-migration-status "${NODE_IDS[$to]}" | grep -q FINISHED; do
    n=$((n + 1))
    [ "$n" -lt "$MIGRATION_POLL_LIMIT" ] || { echo "✗ migration $lo-$hi did not finish" >&2; exit 1; }
    sleep "$MIGRATION_POLL_SECONDS"
  done
  reassign_slots "$from" "$to" "$lo" "$hi"
  push_config "$(render_config)"
  echo "✓ slots $lo-$hi moved ${NODE_IDS[$from]} → ${NODE_IDS[$to]}"
}

# Moves every range back to its canonical owner, one migration per range.
restore_layout() {
  local owner lo hi canonical
  while read -r owner lo hi; do
    canonical=$(printf '%s\n' "$CANONICAL_SLOTS" | awk -v s="$lo" '$2 <= s && s <= $3 { print $1 }')
    [ "$owner" = "$canonical" ] || migrate "$owner" "$canonical" "$lo" "$hi"
  done < <(cat "$SLOTS_FILE")
}

healthy() {
  local i
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    [ "$(cli "$(data_port "$i")" ping 2>/dev/null)" = "PONG" ] || exit 1
  done
  [ "$(cli "$(data_port 0)" cluster shards | grep -c '^online$')" -eq "$NODE_COUNT" ] || exit 1
  # Over TLS, with the lane's own authority: the handshake is the check.
  [ "$(tls_cli ping 2>/dev/null)" = "PONG" ] || exit 1
}

reset() {
  restore_layout
  # Every range is back with its canonical owner, so their union IS the
  # canonical layout; rewriting it collapses the fragments migrations left.
  printf '%s\n' "$CANONICAL_SLOTS" >"$SLOTS_FILE"
  push_config "$(render_config)"
  local p
  for p in "${PRIMARIES[@]}"; do cli "$(data_port "$p")" flushall >/dev/null; done
  admin "$(admin_port "$TLS_NODE")" flushall >/dev/null
  echo "✓ dragonfly cluster flushed and restored to the canonical layout"
}

main() {
case "${1:-serve}" in
  serve)
    start_nodes
    start_tls_node
    wait_for_nodes
    bootstrap
    echo "✓ dragonfly cluster ready on $HOST:$(data_port 0)-$(data_port $((NODE_COUNT - 1))), TLS node on $(data_port "$TLS_NODE")"
    # A node that dies takes the cluster with it: exit so compose reports it
    # rather than serving a partial cluster as healthy.
    wait -n
    echo "✗ a dragonfly node exited" >&2
    exit 1
    ;;
  healthy) healthy ;;
  migrate) migrate "$2" "$3" "$4" "$5" ;;
  reset) reset ;;
  *) echo "usage: $0 serve|healthy|migrate <from> <to> <lo> <hi>|reset" >&2; exit 2 ;;
esac
}

# Sourced by scripts/dragonfly_cluster_test.sh for its pure functions;
# executed as the container entrypoint and by `docker compose exec`. An `if`
# rather than `&&` so a source returns 0 -- `set -e` is already on by then,
# and a sourced file ending in a false list would abort the sourcing shell.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
