// The preference keys the getting-started surfaces read and write. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Mirrors the server registry in rustd/crates/afd_tenant/src/preference/mod.rs —
// the keys ARE the wire strings, so this const and that `PrefKey` must stay in lockstep.
export const PREFERENCE_KEY = {
  DISMISSED: "getting_started_dismissed",
  COLLAPSED: "getting_started_collapsed",
  CLI_TICKED: "getting_started_cli_ticked",
} as const;
export type PreferenceKey = (typeof PREFERENCE_KEY)[keyof typeof PREFERENCE_KEY];
