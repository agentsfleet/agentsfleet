import { request } from "./client";
import type { PreferenceKey } from "./preferences-types";

// Client for GET /v1/workspaces/{ws}/preferences and
// PUT …/preferences/{pref_key}; the keys live in preferences-types.ts.
// The whole preference bag — an object keyed by preference key. Values are the
// opaque JSON the client wrote; onboarding only ever writes booleans.
export type PreferenceBag = Record<string, unknown>;

// Write one preference. The value is sent as the raw request body (the value IS
// the body, per the endpoint). Throws on failure — the caller decides how to
// degrade, because the fail-open direction depends on the key: a failed dismiss
// must leave onboarding SHOWING, so the widget keeps its pre-action state and
// surfaces a retry rather than optimistically hiding. Reads go through the
// consolidated onboarding endpoint (lib/api/onboarding.ts), not a preferences
// GET — this module is now write-only on the client.
export async function putPreference(
  workspaceId: string,
  key: PreferenceKey,
  value: unknown,
  token: string,
): Promise<PreferenceBag> {
  const res = await request<{ prefs?: PreferenceBag }>(
    `/v1/workspaces/${workspaceId}/preferences/${key}`,
    { method: "PUT", body: JSON.stringify(value) },
    token,
  );
  return res.prefs ?? {};
}
