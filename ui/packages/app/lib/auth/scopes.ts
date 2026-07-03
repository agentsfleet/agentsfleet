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
