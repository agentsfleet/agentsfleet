# The release the integration lane and the local stack run (docker-compose.yml,
# compose.dev-infra.yml). A bus whose deployed version drifts from the tested
# one is a bus nobody has tested.
FROM nats:2.15.2-alpine

# One credential per environment (Indy, M244 §8): the node takes its user and
# password from the same `nats-<env>/api-url` item the daemon dials with, so a
# rotation is one vault edit. Routes join through the app's private DNS name;
# each node gossips the rest of the mesh, so a new machine needs no list. The
# client and route addresses advertised are the machine's own 6PN address, so
# a daemon learns every peer and reconnects to one when its node stops.
# JetStream stays off: core NATS only (M244 Invariant 1).
#
# A shell entry point on purpose: the exec form would not expand the variables.
ENTRYPOINT ["/bin/sh", "-c", "\
  : \"${NATS_URL:?NATS_URL is unset: stage nats-<env>/api-url on this app}\"; \
  : \"${FLY_PRIVATE_IP:?FLY_PRIVATE_IP is empty: peers could not reach this node}\"; \
  credentials=\"${NATS_URL#nats://}\"; credentials=\"${credentials%%@*}\"; \
  exec nats-server \
    --name \"$FLY_MACHINE_ID\" \
    --addr :: --port 4222 --http_port 8222 \
    --user \"${credentials%%:*}\" --pass \"${credentials#*:}\" \
    --client_advertise \"[$FLY_PRIVATE_IP]:4222\" \
    --cluster_name \"$FLY_APP_NAME\" \
    --cluster \"nats://[::]:6222\" \
    --cluster_advertise \"[$FLY_PRIVATE_IP]:6222\" \
    --routes \"nats://$FLY_APP_NAME.internal:6222\""]
