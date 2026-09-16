# Pinned to the release the probe, the prototype proofs and the integration
# lane all ran against — the same tag docker-compose.yml carries. A datastore
# whose deployed version drifts from the tested one is a datastore nobody has
# tested.
FROM docker.dragonflydb.io/dragonflydb/dragonfly:v1.40.2

# The same four-process cluster the integration lane runs, from the same
# script. Copied rather than mounted: compose can bind-mount from the
# worktree, a Fly image has no worktree to mount from, and a second copy of
# the bootstrap logic is the one thing this must not become.
COPY scripts/dragonfly-cluster.sh /usr/local/bin/dragonfly-cluster.sh

# Nodes announce the machine's own 6PN address, not loopback. agentsfleetd-dev
# is a different Fly app, so a MOVED reply naming 127.0.0.1 would send it back
# to itself; FLY_PRIVATE_IP is the address dragonfly-dev.internal resolves to.
# --bind=:: accepts that private IPv6 while dual-stack keeps loopback working
# for the script's own redis-cli and for the loopback-only admin ports.
#
# The exec form would not expand either variable, so this is a shell entry
# point on purpose.
ENTRYPOINT ["/bin/bash", "-c", "export DRAGONFLY_ANNOUNCE_IP=\"$FLY_PRIVATE_IP\" DRAGONFLY_BIND=:: && exec /bin/bash /usr/local/bin/dragonfly-cluster.sh serve"]
