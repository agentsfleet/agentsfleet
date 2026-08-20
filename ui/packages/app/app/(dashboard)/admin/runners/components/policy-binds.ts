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

/** `protocol_bind.BASELINE_RO_PATHS` — the mounts the daemon already binds.
 *
 * `/etc` and `/opt` left this list: they carried the host account database and
 * the daemon's own control-plane token into every lease, and nothing a lease
 * runs reads them. The individual `/etc` files a lease DOES read are named
 * instead. The executable and library trees stayed, because the engine's model
 * transport spawns `curl`. */
/* DANGER_HOST_ — every host path a lease can reach. Names mirror
 * `protocol_bind_paths.zig` (RULE UFS: cross-runtime constants share a name).
 * Every baseline entry carries the prefix because every one is HOST filesystem
 * mounted into a sandbox running prompt-injectable agent code; grep
 * `DANGER_HOST_` to see the whole lease-reachable surface at once. */

/** TLS trust store — the only filesystem input a credentialed dial needs.
 * PLATFORM ASSUMPTION: Debian-family and Alpine location; Red Hat family keeps
 * its bundle under `/etc/pki/tls/certs`, which is not carried. */
export const DANGER_HOST_SSL_CERTS = "/etc/ssl/certs";

/** Name resolution. Read by the libc resolver inside a lease; without these
 * every hostname fails and no model is reachable. `/etc/resolv.conf` is a
 * symlink into the resolver directory, never a bind of its own.
 *
 * PLATFORM ASSUMPTION: the systemd-resolved layout, present on the deploy
 * target and absent on Alpine, containers, and NetworkManager hosts. Absence
 * does not degrade gracefully — the directory bind is skipped but the symlink
 * is still emitted, leaving a dangling `/etc/resolv.conf` and no resolution.
 * `nsswitch.conf` is glibc-only and simply absent under musl. */
export const DANGER_HOST_NETWORK_RESOLVER_DIR = "/run/systemd/resolve";
export const DANGER_HOST_NETWORK_HOSTS = "/etc/hosts";
export const DANGER_HOST_NETWORK_NSSWITCH = "/etc/nsswitch.conf";

/** System core: the host's executables and shared libraries — the widest
 * surface here and the one carrying real risk. `/usr` alone is tens of
 * thousands of files, all readable and executable by agent code in a lease.
 * Bound only because the engine's model transport spawns `curl`; without them
 * every lease dies at `execvp` before its first model call. They buy a working
 * product, not security, and they leave when the transport needs no
 * subprocess. */
export const DANGER_HOST_SYSTEM_CORE = ["/usr", "/lib", "/lib64", "/bin", "/sbin"];

export const BASELINE_RO_PATHS = [
  DANGER_HOST_SSL_CERTS,
  DANGER_HOST_NETWORK_RESOLVER_DIR,
  DANGER_HOST_NETWORK_HOSTS,
  DANGER_HOST_NETWORK_NSSWITCH,
  ...DANGER_HOST_SYSTEM_CORE,
];

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
  // The deploy writes the runner token to `/opt/agentsfleet/.env`; the entry
  // above named a directory the token does not live in.
  "/opt/agentsfleet",
  // Refused as a tree now that the baseline binds only individual files under
  // it — otherwise an operator bind could reach `/etc/shadow`, or replace the
  // `/etc/resolv.conf` symlink and redirect name resolution for every lease.
  "/etc",
];

export const BIND_MODES = [BIND_MODE.read_only, BIND_MODE.read_write] as const;

export const BIND_MODE_LABELS: Record<BindMode, string> = {
  read_only: "Read-only",
  read_write: "Read-write",
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
  if (path.length === 0) return "Name a host path to mount";
  if (!path.startsWith("/")) return "Mount paths are absolute — start with /";
  if (path.length < 2 || path.length > MAX_BIND_PATH_LEN) {
    return `Between 2 and ${MAX_BIND_PATH_LEN} characters`;
  }
  if (path.endsWith("/")) return "Drop the trailing slash — one spelling per path";
  if (path.includes("\0")) return "Mount paths cannot contain a NUL byte";
  if (path.split("/").some((seg) => seg === "..")) return "No .. segment — a mount cannot escape the path it names";
  for (const p of BASELINE_RO_PATHS) {
    if (pathsOverlap(path, p)) return `The daemon already mounts ${p} — an assignment can only add paths, never re-mode one`;
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
