# Platform operations fleet fixture

Tests use this bundle to check library upload and fleet installation.

`SKILL.md` defines the fleet instructions. `TRIGGER.md` defines triggers, tools, network access, and spending limits.

## Current install flow

Upload this bundle through `POST /v1/workspaces/<WORKSPACE_ID>/fleet-libraries` first. Use the returned identifier with the client.

```bash
agentsfleet secret create fly --data='{"api_token":"af_test_00000000"}'
agentsfleet secret create upstash --data='{"api_token":"af_test_00000000"}'
agentsfleet secret create slack --data='{"bot_token":"af_test_00000000"}'
agentsfleet install --library <LIBRARY_ID>
agentsfleet steer <FLEET_ID> "morning health check"
```

`<WORKSPACE_ID>` comes from `agentsfleet workspace show`. `<LIBRARY_ID>` comes from the upload response.

`<FLEET_ID>` comes from the install response. Replace the fake credentials before using the bundle outside tests.
