# First Setup or Clean Rebuild

Use this route for the first installation or after a wipe. Run the steps in
order; resume at the first step whose evidence is missing.

The request `Set up agentsfleet development and production from scratch` routes
here. The Agent begins with read-only account and repository checks, reports
everything the Human must create, and waits for each required approval.

| Step | Human action | Agent action | Pipeline verification | Required evidence |
|---|---|---|---|---|
| 01 | Create accounts, approve billing, enter secrets, and configure GitHub. | Synchronize and verify Vercel settings. | None. | Provider account names and successful Vercel check. |
| 02 | Apply the Tailscale policy. | Run bootstrap and deployment preflight. | Deployment workflows repeat the deployment gate. | Green preflight output for development and production. |
| 03 | Create billed and console-only provider resources. | Create Fly.io apps, allocate approved egress, and record repository variables. | None. | Resource names, non-secret identifiers, and green deployment preflight. |
| 04 | Repair Vercel or DNS ownership if needed. | Start and monitor the first development deployment. | Build and deploy the development API, tunnel, and dashboard. | Green workflow URL with runner lanes intentionally skipped. |
| 05 | Reinstall the development host, join Tailscale, register providers, and create the runner token. | Prepare the host and synchronize provider configuration. | Deploy and verify the development runner. | Green runner job, service check, cgroup delegation check, and API readiness. |
| 06 | Confirm provider-console checks and approve external writes. | Apply allowlisting and observability, then record all evidence. | Run development browser and command-line acceptance. | Every development job and operational verification is green. |
| 07 | Approve the exact production release tag and repair domains if needed. | Push the approved tag and monitor the release. | Build, publish, and deploy the production control plane. | Green release URL with runner fleet still disabled. |
| 08 | Reinstall production hosts, join Tailscale, register providers, and create runner tokens. | Prepare hosts, synchronize provider configuration, and record inventory. | No runner rollout until step 09. | Every host preparation gate is green and inventory is complete. |
| 09 | Approve the remaining fleet after the canary passes. | Start and monitor the final release run; verify providers. | Deploy canary, deploy approved fleet, run acceptance, and promote npm `latest`. | Every production job, provider check, domain, and package check is green. |

## Human approval points

The Agent stops before:

1. Reading a vault interactively.
2. Applying an external provider change.
3. Changing a live host or restarting work in flight.
4. Creating or pushing a production release tag.
5. Approving the production runner fleet.
6. Running a destructive teardown.

## Resume rule

Evidence, not directory number, determines where to resume. The Agent reads each
preceding step's `Required result` section and verifies the cited command or
Pipeline URL. Missing, red, or skipped evidence means that step is incomplete.

For an existing installation, return to the [route selector](../README.md)
instead of replaying this sequence.
