// The runner vocabulary the admin runner dialogs, tables and status views share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Assignable isolation strength — mirrors `protocol.SandboxTier` verbatim
// (UFS: the tag names are the wire shape). Only tiers with real enforcement
// are assignable (the Seatbelt tier was removed — it never had enforcement
// code, and a tier that cannot be applied must not be assignable). `dev_none`
// is dev-only; a release daemon refuses it at boot.
export const SANDBOX_TIERS = ["landlock_full", "container_nested", "dev_none"] as const;

export type SandboxTier = (typeof SANDBOX_TIERS)[number];

// Operator-facing labels for the assignable isolation tiers — the raw enum
// tags are the wire shape (sent verbatim), these are the human strings the
// dropdown and list render. Keyed so a new tier can't be added without a label.
export const SANDBOX_TIER_LABELS: Record<SandboxTier, string> = {
  landlock_full: "Landlock",
  container_nested: "Nested container",
  dev_none: "None",
};

// How an operator-added path is mounted — mirrors `protocol_bind.BindMode`.
// An entry that names no mode is read-only, so access never widens by omission.
export const BIND_MODE = {
  read_only: "read_only",
  read_write: "read_write",
} as const;

export type BindMode = (typeof BIND_MODE)[keyof typeof BIND_MODE];

export const RUNNER_ADMIN_STATE = {
  active: "active",
  cordoned: "cordoned",
  draining: "draining",
  drained: "drained",
  revoked: "revoked",
} as const;

export type RunnerAdminState = (typeof RUNNER_ADMIN_STATE)[keyof typeof RUNNER_ADMIN_STATE];

export const RUNNER_EVENT_TYPES = [
  "runner_registered",
  "runner_online",
  "runner_offline",
  "lease_acquired",
  "lease_released",
  "runner_cordoned",
  "runner_draining",
  "runner_drained",
  "runner_revoked",
  "runner_policy_assigned",
] as const;

export type RunnerEventType = (typeof RUNNER_EVENT_TYPES)[number];

// The lifecycle subset Activity renders: every tag EXCEPT the two per-work
// records, which the Leases table already states once each with an outcome.
// One exported constant, consumed verbatim by the Activity caller (Invariant:
// lifecycle and work events never mix in that feed).
export const RUNNER_LIFECYCLE_EVENT_TYPES = [
  "runner_registered",
  "runner_online",
  "runner_offline",
  "runner_cordoned",
  "runner_draining",
  "runner_drained",
  "runner_revoked",
  "runner_policy_assigned",
] as const satisfies readonly RunnerEventType[];

/// The "never contacted" sentinel for `last_seen_at`, mirroring
/// `protocol.RUNNER_LAST_SEEN_NEVER` — same name across both runtimes so the
/// pair stays greppable. A runner is minted with this at registration and
/// carries it until its first heartbeat, so it is a real state, not a null.
export const RUNNER_LAST_SEEN_NEVER = 0;

// host_id is free-form but bounded by the backend; deriving HOST_ID_REGEX from
// HOST_ID_MAX keeps the form in step with `register.zig`'s MAX_HOST_ID_LEN as a
// single source — the bound lives in exactly one place.
export const HOST_ID_MAX = 256;
export const HOST_ID_REGEX = new RegExp(`^[A-Za-z0-9_.-]{1,${HOST_ID_MAX}}$`);
export const LABEL_REGEX = /^[A-Za-z0-9_.-]{1,64}$/;

// Settled server-side into one closed tag; the client never re-derives an
// outcome from raw statuses (the two surfaces cannot drift on what expired means).
export const LEASE_OUTCOME = {
  running: "running",
  succeeded: "succeeded",
  failed: "failed",
  expired: "expired",
  unknown: "unknown",
} as const;
export type LeaseOutcome = (typeof LEASE_OUTCOME)[keyof typeof LEASE_OUTCOME];

/**
 * Split the free-form labels field (comma-separated) into a deduped, validated
 * set. Returns the first offending label as an error so the form can surface it;
 * an empty/whitespace-only input is a valid empty set.
 */
export function parseLabels(raw: string): { labels: string[]; error: string | null } {
  const parts = raw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  const seen = new Set<string>();
  for (const p of parts) {
    if (!LABEL_REGEX.test(p)) {
      return { labels: [], error: `Label "${p}" must be 1–64 chars: letters, digits, dot, hyphen, underscore` };
    }
    seen.add(p);
  }
  return { labels: [...seen], error: null };
}

// One-line descriptions for the isolation-mode OptionCard picker. Keyed by
// the same SandboxTier so a new tier can't be added without one. Mirrors
// docs/architecture/runner_fleet.md §Sandbox tiers — do not restate from the
// label alone; that table is the source of truth for what each tier means.
export const SANDBOX_TIER_DESCRIPTIONS: Record<SandboxTier, string> = {
  landlock_full: "Linux kernel level sandboxing with full isolation.",
  container_nested: "Runs inside a container on a Linux host or VM.",
  dev_none: "No sandbox — for development only.",
};

// Egress posture assigned per runner — mirrors `protocol.NetworkPolicy` verbatim
// (UFS: the tag names are the wire shape). `allow_list_egress` marks the runner
// degraded until its enforcement ships; the dialog says so when offering it.
export const NETWORK_POLICIES = ["allow_all", "deny_all_egress", "allow_list_egress"] as const;

export type NetworkPolicy = (typeof NETWORK_POLICIES)[number];

// Operator-facing labels for the egress postures — the raw tags are the wire
// shape, these are the strings the select renders. Keyed so a new mode can't
// be added without a label.
export const NETWORK_POLICY_LABELS: Record<NetworkPolicy, string> = {
  allow_all: "Allow all egress",
  deny_all_egress: "No egress",
  allow_list_egress: "Allowlist egress",
};

// One-line descriptions for the egress select. `allow_list_egress` says
// exactly what assigning it does today: the host cannot enforce it yet, so the
// runner reads degraded and refuses work until that enforcement ships.
export const NETWORK_POLICY_DESCRIPTIONS: Record<NetworkPolicy, string> = {
  allow_all: "All outbound traffic allowed — the interim open posture.",
  deny_all_egress: "No outbound network at all.",
  allow_list_egress: "Outbound only to an approved list — enforcement unshipped; a runner assigned this is degraded.",
};

// Enrollment defaults for the policy fields. Network defaults to the explicit
// interim open posture — defaulting to the strict allowlist before its
// enforcement ships would degrade every new runner. `DEFAULT_WORKER_COUNT`,
// `MIN_WORKER_COUNT`, and `MAX_WORKER_COUNT` mirror their `protocol.*`
// namesakes (UFS cross-runtime names); the server clamps into the same bounds.
export const DEFAULT_ASSIGNED_NETWORK_POLICY: NetworkPolicy = "allow_all";

export const DEFAULT_WORKER_COUNT = 1;

export const MIN_WORKER_COUNT = 1;

export const MAX_WORKER_COUNT = 64;

// Registry allowlist entries are host[:port] names, one per comma. The server
// enforces the same grammar and cap (`protocol.MAX_REGISTRY_ENTRIES`, UFS
// cross-runtime name) — the dialog refuses first so the operator hears it
// in-form rather than as a 400.
export const REGISTRY_HOST_REGEX = /^[A-Za-z0-9_.-]{1,253}(:[0-9]{1,5})?$/;

export const MAX_REGISTRY_ENTRIES = 32;

/**
 * Split the free-form registry allowlist field (comma-separated) into a
 * deduped, validated set — the registry twin of `parseLabels`. An
 * empty/whitespace-only input is a valid empty set (the runner substitutes its
 * default registry set).
 */
export function parseRegistryAllowlist(raw: string): { hosts: string[]; error: string | null } {
  const parts = raw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  const seen = new Set<string>();
  for (const p of parts) {
    if (!REGISTRY_HOST_REGEX.test(p)) {
      return { hosts: [], error: `Registry "${p}" must be a host name, optionally with a port` };
    }
    seen.add(p);
  }
  if (seen.size > MAX_REGISTRY_ENTRIES) {
    return { hosts: [], error: `At most ${MAX_REGISTRY_ENTRIES} registries per runner` };
  }
  return { hosts: [...seen], error: null };
}

// One operator-assigned sandbox mount — mirrors `protocol_bind.ExtraBind`.
// `read_write` is a real boundary widening: tenant agent code can then modify
// host state outside its workspace on every lease that runner takes, so the
// page renders it differently from a plain row.
export interface ExtraBind {
  path: string;
  mode?: BindMode;
  note?: string;
}

// The policy the operator assigns to a runner — mirrors `protocol.AssignedPolicy`
// verbatim. The host applies exactly this; it never declares its own.
export interface AssignedPolicy {
  sandbox_tier: SandboxTier;
  network_policy: NetworkPolicy;
  registry_allowlist: string[];
  worker_count: number;
  // Paths bound IN ADDITION to the daemon-owned baseline. Optional on the wire:
  // a runner enrolled before the column existed sends nothing, which reads as
  // "baseline only" rather than as a missing assignment.
  extra_binds?: ExtraBind[];
}

// The PATCH verbs the daemon serves — mirrors `protocol.RunnerAdminAction`.
export const RUNNER_ADMIN_ACTION = {
  cordon: "cordon",
  drain: "drain",
  revoke: "revoke",
  self_test: "self_test",
} as const;

export type RunnerAdminAction = (typeof RUNNER_ADMIN_ACTION)[keyof typeof RUNNER_ADMIN_ACTION];

// The subset that moves `admin_state`. `self_test` records a request and moves
// nothing, so the transition map and `actionsFor` key on THIS narrower type —
// same reasoning that keeps Delete out of ACTION_CONFIG. Widening the map to
// every wire verb would make a transition table answer for a non-transition.
export const RUNNER_ADMIN_ACTIONS = [
  RUNNER_ADMIN_ACTION.cordon,
  RUNNER_ADMIN_ACTION.drain,
  RUNNER_ADMIN_ACTION.revoke,
] as const;

export type RunnerStateAction = (typeof RUNNER_ADMIN_ACTIONS)[number];
