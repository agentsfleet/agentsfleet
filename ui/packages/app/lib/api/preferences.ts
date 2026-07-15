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

// The value cap the server enforces (UZ-PREFS-002). Mirrors
// MAX_PREF_VALUE_BYTES in the Zig store. These preferences are single booleans,
// nowhere near it, but the client should not send what the server will refuse.
export const MAX_PREF_VALUE_BYTES = 1024;

// The whole preference bag — an object keyed by preference key. Values are the
// opaque JSON the client wrote; onboarding only ever writes booleans.
export type PreferenceBag = Record<string, unknown>;

// Fail-open read: a failed or empty response yields an empty bag, so a read
// failure can never HIDE onboarding (Invariant 3). "I couldn't read your
// preferences" must look exactly like "you have set none" — both show the
// checklist. This is why the read swallows and returns {} rather than throwing.
export async function getPreferences(
  workspaceId: string,
  token: string,
): Promise<PreferenceBag> {
  try {
    const res = await request<{ prefs?: PreferenceBag }>(
      `/v1/workspaces/${workspaceId}/preferences`,
      { method: "GET" },
      token,
    );
    return res.prefs ?? {};
  } catch {
    return {};
  }
}

// Write one preference. The value is sent as the raw request body (the value IS
// the body, per the endpoint). Throws on failure — the caller decides how to
// degrade, because the fail-open direction depends on the key: a failed dismiss
// must leave onboarding SHOWING, so the widget keeps its pre-action state and
// surfaces a retry rather than optimistically hiding.
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

// Read a boolean preference from a bag with prototype-safe membership: the key
// comes off the wire, so `in` would walk the prototype chain and treat
// `constructor`/`toString` as present (RULE PTK). Missing or non-true → false.
export function prefIsTrue(bag: PreferenceBag, key: PreferenceKey): boolean {
  if (!Object.hasOwn(bag, key)) return false;
  return bag[key] === true;
}
