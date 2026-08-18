import { BIND_MODE, type BindMode, type ExtraBind } from "@/lib/api/runners";

// The extra-bind half of the assignment form: bounds, grammar, and the
// form-row <-> wire conversion. Mirrors `protocol_bind.zig` so the dialog
// refuses an entry in-form rather than as a 400, and split from PolicyFields
// so neither file carries both the four scalar fields and the bind list.
//
// The rules here are a MIRROR, never the enforcement. `extraBindsValid` on the
// daemon is the boundary; this exists so an operator hears the reason next to
// the row they typed.

/** `protocol_bind.MAX_EXTRA_BINDS` (UFS cross-runtime name). */
export const MAX_EXTRA_BINDS = 16;
/** `protocol_bind.MAX_BIND_PATH_LEN` — PATH_MAX on Linux. */
export const MAX_BIND_PATH_LEN = 4096;
/** `protocol_bind.MAX_BIND_NOTE_LEN` — one line of operator intent. */
export const MAX_BIND_NOTE_LEN = 200;

/** `protocol_bind.BASELINE_RO_PATHS` — the mounts the daemon already binds. */
export const BASELINE_RO_PATHS = ["/etc", "/lib", "/lib64", "/bin", "/sbin", "/opt", "/run/systemd/resolve"];

/** `protocol_bind.SENSITIVE_PATHS` — the sandbox's own floor plus the host
 * surfaces where a writable mount is host control rather than a repair. */
export const SENSITIVE_PATHS = [
  "/usr",
  "/proc",
  "/dev",
  "/tmp",
  "/root",
  "/home",
  "/boot",
  "/sys",
  "/run",
  "/var/run",
  "/var/lib/agentsfleet",
];

export const BIND_MODES = [BIND_MODE.read_only, BIND_MODE.read_write] as const;

export const BIND_MODE_LABELS: Record<BindMode, string> = {
  read_only: "Read-only",
  read_write: "Read-write",
};

// Named on the control, not buried in a description: read_write lets tenant
// agent code modify host state outside its workspace on every lease this
// runner takes.
export const BIND_MODE_DESCRIPTIONS: Record<BindMode, string> = {
  read_only: "The sandbox can read the path. The host copy is never modified.",
  read_write: "The sandbox can write through to the host path. Widens the isolation boundary for every lease.",
};

/** One editable row. All-strings plus the mode enum, so react-hook-form's
 * input and output types stay identical (the worker_count convention). */
export interface BindFormRow {
  path: string;
  mode: BindMode;
  note: string;
}

export const BIND_ROW_DEFAULT: BindFormRow = { path: "", mode: BIND_MODE.read_only, note: "" };

/** True when two absolute paths name the same mount or one contains the other.
 * Segment-aware: `/etc` contains `/etc/ssl` but not `/etcetera`, so a plain
 * prefix compare would refuse legitimate paths. Mirrors `pathsOverlap`. */
export function pathsOverlap(a: string, b: string): boolean {
  if (a === b) return true;
  return containsPath(a, b) || containsPath(b, a);
}

function containsPath(parent: string, child: string): boolean {
  if (child.length <= parent.length) return false;
  if (!child.startsWith(parent)) return false;
  return child[parent.length] === "/";
}

/**
 * The reason this path cannot be bound, or null when it is well-formed and
 * additive. One message per rule so the row says which rule it broke —
 * "invalid path" would send the operator back to the daemon's source.
 */
export function bindPathIssue(path: string): string | null {
  if (path.length === 0) return "Name a host path to bind";
  if (!path.startsWith("/")) return "Bind paths are absolute — start with /";
  if (path.length < 2 || path.length > MAX_BIND_PATH_LEN) {
    return `Between 2 and ${MAX_BIND_PATH_LEN} characters`;
  }
  if (path.endsWith("/")) return "Drop the trailing slash — one spelling per path";
  if (path.includes("\0")) return "Bind paths cannot contain a NUL byte";
  if (path.split("/").some((seg) => seg === "..")) return "No .. segment — a bind cannot escape the path it names";
  for (const p of BASELINE_RO_PATHS) {
    if (pathsOverlap(path, p)) return `The daemon already binds ${p} — an assignment can only add paths, never re-mode one`;
  }
  for (const p of SENSITIVE_PATHS) {
    if (pathsOverlap(path, p)) return `${p} is the sandbox's own floor or host control — it cannot be bound`;
  }
  return null;
}

/** The reason this note cannot be stored, or null. */
export function bindNoteIssue(note: string): string | null {
  return note.length > MAX_BIND_NOTE_LEN ? `At most ${MAX_BIND_NOTE_LEN} characters` : null;
}

/**
 * Form rows → the wire list. Empty rows are dropped rather than refused: an
 * operator who opens a row and changes their mind should be able to save.
 * `note` is omitted when blank so the stored assignment carries no empty keys.
 */
export function bindsFromForm(rows: BindFormRow[]): ExtraBind[] {
  return rows
    .map((r) => ({ ...r, path: r.path.trim(), note: r.note.trim() }))
    .filter((r) => r.path.length > 0)
    .map((r) => (r.note.length > 0 ? { path: r.path, mode: r.mode, note: r.note } : { path: r.path, mode: r.mode }));
}

/** A stored assignment's binds → form rows. An entry that names no mode is
 * read-only, matching the wire default — access never widens by omission. */
export function formFromBinds(binds: ExtraBind[] | undefined): BindFormRow[] {
  return (binds ?? []).map((b) => ({
    path: b.path,
    mode: b.mode ?? BIND_MODE.read_only,
    note: b.note ?? "",
  }));
}
