/**
 * Svix HMAC-SHA256 signing for outbound webhook posts.
 *
 * Mirrors the reference shape used by agentsfleetd's identity-events handler
 * (src/http/handlers/auth/identity_events_clerk.zig) and the Svix spec:
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
  "svix-id": string;
  "svix-timestamp": string;
  "svix-signature": string;
}

function decodeWhsec(secret: string): Buffer {
  const cleaned = secret.startsWith("whsec_") ? secret.slice("whsec_".length) : secret;
  return Buffer.from(cleaned, "base64");
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
