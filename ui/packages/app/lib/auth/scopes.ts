/**
 * Operator scope strings — a verbatim mirror of the backend's per-route scope
 * map in `src/agentsfleetd/http/route_scopes.zig:120-127`. The backend
 * `requireScope` middleware is the authoritative gate (403 `UZ-AUTH-022` on a
 * missing scope); these constants drive the dashboard's defence-in-depth checks
 * so the two never drift apart by hand-typed string.
 *
 * This module is intentionally dependency-free (no `@clerk/nextjs/server`
 * import) so it is safe to import from both server components/actions and the
 * client `Shell` nav — unlike `platform.ts`, which reads the live session.
 */
export const SCOPE = {
  /** View the runner fleet — `GET /v1/fleets/runners`, runner activity. */
  RUNNER_READ: "runner:read",
  /** Enroll a runner — `POST /v1/runners`. */
  RUNNER_ENROLL: "runner:enroll",
  /** Cordon/patch a runner's admin state — `PATCH …/runners/{id}`. */
  RUNNER_WRITE: "runner:write",
  /** Read the model catalogue — `GET` on the admin models surface. */
  MODEL_READ: "model:read",
  /** Mutate the model catalogue / platform defaults — non-GET admin models. */
  MODEL_ADMIN: "model:admin",
} as const;

export type Scope = (typeof SCOPE)[keyof typeof SCOPE];

// Downward closure of the `read < write < admin` ladder — a verbatim mirror of
// the backend `HIERARCHY` table (`src/agentsfleetd/auth/scopes.zig`). The
// backend expands a held scope to this closure at parse time, so a token
// provisioned with the documented operator set (which carries `runner:write` /
// `model:admin`, not the `:read` rungs — docs/AUTH.md §Manually-provisioned)
// still passes a `:read` gate. The UI mirrors it so `hasScope` matches the
// backend's authorization decision exactly, rather than hiding a surface the
// backend would allow. Each key lists its FULL closure (flattened, like the
// backend table), so a single lookup suffices — no transitive walk needed.
const SCOPE_INCLUDES: Readonly<Record<string, readonly string[]>> = {
  "fleet:admin": ["fleet:write", "fleet:read"],
  "fleet:write": ["fleet:read"],
  "credential:write": ["credential:read"],
  "apikey:admin": ["apikey:write", "apikey:read"],
  "apikey:write": ["apikey:read"],
  "fleetkey:write": ["fleetkey:read"],
  "grant:write": ["grant:read"],
  "connector:write": ["connector:read"],
  "model:admin": ["model:read"],
  "platform-key:admin": ["platform-key:read"],
  "runner:write": ["runner:read"],
  "approval:resolve": ["approval:read"],
};

/**
 * Expand a held scope set to its downward closure, mirroring the backend's
 * `parseClaim`. A token holding `model:admin` therefore satisfies a
 * `model:read` check — the same decision the backend `requireScope` makes.
 */
export function expandScopes(held: Iterable<string>): Set<string> {
  const out = new Set<string>();
  for (const scope of held) {
    out.add(scope);
    for (const sub of SCOPE_INCLUDES[scope] ?? []) out.add(sub);
  }
  return out;
}
