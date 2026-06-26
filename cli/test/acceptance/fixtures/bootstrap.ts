/**
 * Tenant-bootstrap helper for the CLI acceptance harness.
 *
 * Replays Clerk's `user.created` webhook against agentsfleetd's
 * POST /v1/auth/identity-events/clerk handler so the fixture identity has a
 * tenant row, default workspace, and starter credit before any spec hydrates
 * `/v1/tenants/me/workspaces`. TS twin of the dashboard suite's
 * `ui/packages/app/tests/e2e/acceptance/fixtures/bootstrap.ts`.
 *
 * Why this lives in the CLI suite now: the CLI specs used to rely on the
 * dashboard suite's globalSetup having bootstrapped the shared fixture first.
 * In CI both suites only `needs: verify-dev` and run in parallel (no ordering),
 * and a dev-DB reset wipes the workspace with no re-provision — so the CLI
 * suite must bootstrap its own fixture. `attachJwt` calls this after
 * provisioning the Clerk user, making every spec self-sufficient.
 *
 * Idempotent: replaying `user.created` for an existing oidc_subject returns
 * 200 `created:false` with no new rows, so it is safe on every mint.
 */
import { ACCEPTANCE_TARGET_ENV } from "./constants.ts";
import { newMsgId, signSvix } from "./svix.ts";

interface UserCreatedPayload {
  readonly type: "user.created";
  readonly data: {
    readonly id: string;
    readonly email_addresses: ReadonlyArray<{ id: string; email_address: string }>;
    readonly primary_email_address_id: string;
    readonly first_name: string;
    readonly last_name: string;
  };
}

export interface BootstrapFixtureOptions {
  readonly clerkUserId: string;
  readonly email: string;
}

function deriveFirstName(email: string): string {
  return /admin/i.test(email) ? "Admin" : "Regular";
}

function buildPayload(opts: BootstrapFixtureOptions): UserCreatedPayload {
  return {
    type: "user.created",
    data: {
      id: opts.clerkUserId,
      email_addresses: [{ id: "idn_x", email_address: opts.email }],
      primary_email_address_id: "idn_x",
      first_name: deriveFirstName(opts.email),
      last_name: "Fixture",
    },
  };
}

/**
 * Ensure the fixture's tenant + default workspace exist. No-op only when
 * `AGENTSFLEET_ACCEPTANCE_TARGET` is unset (not an acceptance run). Once a target
 * is set we are mid-acceptance and the bootstrap is REQUIRED: a missing/empty
 * `CLERK_WEBHOOK_SECRET` throws loudly (matching the dashboard suite) rather than
 * silently skipping into a confusing downstream hydration failure.
 */
export async function ensureFixtureTenantBootstrapped(opts: BootstrapFixtureOptions): Promise<void> {
  const secret = process.env.CLERK_WEBHOOK_SECRET;
  const apiUrl = process.env[ACCEPTANCE_TARGET_ENV];
  if (!apiUrl) return; // not an acceptance run — nothing to bootstrap against
  if (!secret) {
    throw new Error(
      `CLERK_WEBHOOK_SECRET unset/empty but ${ACCEPTANCE_TARGET_ENV} is set — the acceptance ` +
        "suite cannot sign the user.created webhook to bootstrap the fixture tenant. " +
        "Set CLERK_WEBHOOK_SECRET (op://VAULT_DEV/clerk-dev/webhook-secret).",
    );
  }

  const body = JSON.stringify(buildPayload(opts));
  const headers = signSvix(secret, newMsgId("msg_cli_bootstrap"), body);
  const res = await fetch(`${apiUrl}/v1/auth/identity-events/clerk`, {
    method: "POST",
    headers: { ...headers, "Content-Type": "application/json" },
    body,
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(
      `fixture tenant bootstrap failed for ${opts.email}: ${res.status} ${res.statusText}\n${detail.slice(0, 300)}`,
    );
  }
  // Drain the body so the connection is released; the payload is unused.
  await res.text().catch(() => "");
}
