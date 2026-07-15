import { request } from "./client";

// Client for GET /v1/workspaces/{ws}/preferences and
// PUT …/preferences/{pref_key}. Mirrors the server registry in
// src/agentsfleetd/state/user_preferences.zig — the keys ARE the wire strings,
// so this const and the Zig enum must stay in lockstep.
export const PREFERENCE_KEY = {
  DISMISSED: "getting_started_dismissed",
  COLLAPSED: "getting_started_collapsed",
  CLI_TICKED: "getting_started_cli_ticked",
} as const;
export type PreferenceKey = (typeof PREFERENCE_KEY)[keyof typeof PREFERENCE_KEY];

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
