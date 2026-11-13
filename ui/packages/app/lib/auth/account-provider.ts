import { z } from "zod";
import { boundedBody } from "@/lib/api/setup-proxy/body";
import { ACCOUNT_PROVIDER_CONFIG as CONFIG } from "./account-config";

const activitySchema = z.object({browser_name: z.string().nullish(), device_type: z.string().nullish(),
  city: z.string().nullish(), country: z.string().nullish(), is_mobile: z.boolean().nullish()});
const sessionSchema = z.object({id: z.string().min(1).max(256), user_id: z.string().min(1).max(256), status: z.string(),
  last_active_at: z.number().nullish(), latest_activity: activitySchema.nullish()});
const pageSchema = z.object({data: z.array(sessionSchema).max(50), total_count: z.number().int().nonnegative()});
export type IdentitySession = z.infer<typeof sessionSchema>;

/** The provider URL is fixed; neither user input nor a redirect can receive the secret. */
export function readProviderSessions(subject: string, offset: number, signal: AbortSignal) {
  const query = new URLSearchParams({user_id: subject, status: "active", limit: "50", offset: String(offset), paginated: "true"});
  return providerRequest(`/sessions?${query}`, pageSchema, signal);
}

export function readProviderSession(id: string, signal: AbortSignal) {
  return providerRequest(`/sessions/${encodeURIComponent(id)}`, sessionSchema, signal);
}

export function revokeProviderSession(id: string, signal: AbortSignal) {
  return providerRequest(`/sessions/${encodeURIComponent(id)}/revoke`, sessionSchema, signal, "POST");
}

async function providerRequest<T>(path: string, schema: z.ZodType<T>, signal: AbortSignal, method = "GET"): Promise<T> {
  const secret = CONFIG.secretKey;
  if (!secret) throw new Error("Account settings are unavailable.");
  signal.throwIfAborted();
  const response = await fetch(`${CONFIG.apiRoot}${path}`, {method, signal, redirect: "error", cache: "no-store",
    headers: {Authorization: `Bearer ${secret}`, Accept: "application/json", "Clerk-API-Version": CONFIG.apiVersion}});
  if (!response.ok) {void response.body?.cancel().catch(() => undefined); throw new Error("The account request was not confirmed.");}
  const bytes = await boundedBody(response, signal, CONFIG.bodyLimit);
  signal.throwIfAborted();
  if (bytes === null) {void response.body?.cancel().catch(() => undefined); throw new Error("The account response exceeds its limit.");}
  return schema.parse(JSON.parse(new TextDecoder().decode(bytes)));
}
