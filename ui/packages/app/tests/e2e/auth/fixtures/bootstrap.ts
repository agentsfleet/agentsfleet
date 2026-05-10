/**
 * Tenant-bootstrap helper for the e2e harness.
 *
 * Replays Clerk's `user.created` webhook against zombied's
 * POST /v1/webhooks/clerk handler so each fixture user has a tenant row,
 * default workspace, and starter credit before any spec runs. The wire shape
 * mirrors the integration test at
 * src/http/handlers/webhooks/clerk_integration_test.zig:82 (happy path)
 * and :348 (replay idempotency — replaying the same user.created returns
 * `created:false` with no new rows).
 *
 * Idempotent: safe to call on every globalSetup; replays return 200 with
 * created:false.
 */
import { newMsgId, signSvix } from "./svix";
import type { ProvisionedUser } from "./clerk-admin";
import { FIXTURE_KEY } from "./constants";

interface UserCreatedPayload {
  type: "user.created";
  data: {
    id: string;
    email_addresses: Array<{ id: string; email_address: string }>;
    primary_email_address_id: string;
    first_name: string;
    last_name: string;
  };
}

function buildPayload(fixture: ProvisionedUser): UserCreatedPayload {
  return {
    type: "user.created",
    data: {
      id: fixture.clerkUserId,
      email_addresses: [{ id: "idn_x", email_address: fixture.email }],
      primary_email_address_id: "idn_x",
      first_name: fixture.key === FIXTURE_KEY.admin ? "Admin" : "Regular",
      last_name: "Fixture",
    },
  };
}

interface BootstrapResponse {
  created: boolean;
  tenant_id: string;
  workspace_name?: string;
}

export async function bootstrapTenant(fixture: ProvisionedUser): Promise<BootstrapResponse> {
  const apiUrl = process.env.NEXT_PUBLIC_API_URL;
  const secret = process.env.CLERK_WEBHOOK_SECRET;
  if (!apiUrl || !secret) {
    throw new Error("NEXT_PUBLIC_API_URL and CLERK_WEBHOOK_SECRET must be set before bootstrap");
  }

  const body = JSON.stringify(buildPayload(fixture));
  const headers = signSvix(secret, newMsgId("msg_e2e_bootstrap"), body);

  const res = await fetch(`${apiUrl}/v1/webhooks/clerk`, {
    method: "POST",
    headers: { ...headers, "Content-Type": "application/json" },
    body,
  });

  if (!res.ok) {
    const detail = await res.text();
    throw new Error(
      `Tenant bootstrap failed for ${fixture.email}: ${res.status} ${res.statusText}\n${detail}`,
    );
  }
  return (await res.json()) as BootstrapResponse;
}
