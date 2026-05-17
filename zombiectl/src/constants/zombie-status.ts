// Zombie lifecycle status values — the wire-level enum the server
// accepts on PATCH /v1/workspaces/{ws}/zombies/{id}. Frozen object
// so destructuring still works while preventing accidental writes.
//
// Mirrors the server-side enum in src/state/zombies.zig (Status).
//
// RULE UFS — every emit site reads from here.

export const ZOMBIE_STATUS = Object.freeze({
  STOPPED: "stopped",
  ACTIVE: "active",
  KILLED: "killed",
});
