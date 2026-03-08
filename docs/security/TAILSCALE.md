# Tailscale Security

## Why This Exists

Application-layer auth is not enough for infrastructure exposure. We still need explicit network-level allowlists so data stores and workers are unreachable from the public internet.

## Decisions

1. API and worker nodes run inside a Tailscale tailnet.
2. ACLs restrict east-west traffic by role and port.
3. Data stores only allow trusted network sources.

## What This Prevents

1. Direct external access to Postgres/Redis.
2. Lateral movement from unauthorized nodes.
3. Worker-plane exposure as a public service.

## Software Setup Steps

1. Join API and worker hosts to the same tailnet.
2. Apply ACL rules for role-to-role traffic.
3. Ensure ingress only exposes API endpoint, not worker or data stores.
4. For managed Postgres/Redis, configure IP allowlists to trusted egress/Tailscale paths.

## Verification

1. External probe to Postgres/Redis should fail.
2. Worker should not be publicly reachable.
3. API should remain reachable only through intended ingress path.
