/**
 * Svix HMAC-SHA256 signing for outbound webhook posts.
 *
 * TS twin of `ui/packages/app/tests/e2e/acceptance/fixtures/svix.ts`. Mirrors
 * agentsfleetd's identity-events handler
 * (src/agentsfleetd/http/handlers/auth/identity_events_clerk.zig) and the Svix
 * spec:
 *   signed_input = `${id}.${timestamp}.${body}`
 *   signature    = base64( HMAC_SHA256(decode_base64(secret_after_whsec_prefix), signed_input) )
 *   header value = `v1,${signature}`
 *
 * Secret format: Clerk's webhook secret is `whsec_<base64>`. The HMAC key is
 * the base64-decoded portion after the `whsec_` prefix.
 *
 * https://docs.svix.com/receiving/verifying-payloads/how
 */
import * as crypto from "node:crypto";

export interface SvixHeaders {
  readonly "svix-id": string;
  readonly "svix-timestamp": string;
  readonly "svix-signature": string;
}

function decodeWhsec(secret: string): Buffer {
  // The agentsfleetd verifier (auth/crypto/svix_verify.zig) returns
  // `.invalid_signature` for any configured secret that does NOT start with
  // `whsec_`. Accepting a bare base64 value here would produce a plausible
  // signature the API always rejects, surfacing as a confusing bootstrap
  // failure for every attachJwt caller — so require the prefix and fail loud.
  if (!secret.startsWith("whsec_")) {
    throw new Error(
      "CLERK_WEBHOOK_SECRET must be a Clerk webhook secret of the form 'whsec_<base64>'; " +
        "got a value without the whsec_ prefix (the agentsfleetd Svix verifier rejects it).",
    );
  }
  return Buffer.from(secret.slice("whsec_".length), "base64");
}

export function signSvix(secret: string, msgId: string, body: string): SvixHeaders {
  const ts = String(Math.floor(Date.now() / 1000));
  const key = decodeWhsec(secret);
  const signedInput = `${msgId}.${ts}.${body}`;
  const sig = crypto.createHmac("sha256", key).update(signedInput).digest("base64");
  return {
    "svix-id": msgId,
    "svix-timestamp": ts,
    "svix-signature": `v1,${sig}`,
  };
}

export function newMsgId(prefix = "msg"): string {
  return `${prefix}_${crypto.randomBytes(8).toString("hex")}`;
}
