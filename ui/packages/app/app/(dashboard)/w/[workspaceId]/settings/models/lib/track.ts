import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import type { TenantProvider } from "@/lib/types";

// One place per Models product event, so every call site emits the same
// shape. captureProductEvent is fire-and-forget + error-safe, so callers never
// await or guard these.

/** A secret was activated as the tenant model — new key, custom endpoint, or switch-list switch. */
export function captureModelActivated(p: Pick<TenantProvider, "provider" | "mode" | "model">): void {
  captureProductEvent(EVENTS.model_added, { provider: p.provider, mode: p.mode, model: p.model });
}

/** Same key, a different model (hero "Change model"). */
export function captureModelChanged(p: Pick<TenantProvider, "provider" | "model">): void {
  captureProductEvent(EVENTS.model_changed, { provider: p.provider, model: p.model });
}

/** The active secret's value was rotated (hero "Replace key"); no secret in the payload. */
export function captureKeyRotated(provider: string): void {
  captureProductEvent(EVENTS.key_rotated, { provider });
}

/** Reverted to platform defaults (hero or switch list); records the provider left behind. */
export function captureProviderReset(fromProvider: string): void {
  captureProductEvent(EVENTS.provider_reset, { from_provider: fromProvider });
}
