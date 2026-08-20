import { describe, expect, it } from "vitest";
import {
  BIND_ROW_DEFAULT,
  MAX_BIND_NOTE_LEN,
  MAX_BIND_PATH_LEN,
  bindNoteIssue,
  bindPathIssue,
  bindsFromForm,
  formFromBinds,
  pathsOverlap,
} from "./policy-binds";

// These rules MIRROR `protocol_bind.extraBindsValid`; the daemon stays the
// boundary. The tests below are the mirror's proof — each one names a rule the
// Zig side enforces, so a drift between the two shows up here rather than as a
// 400 the operator has to decode.

describe("pathsOverlap", () => {
  it("treats the same path as an overlap", () => {
    expect(pathsOverlap("/etc", "/etc")).toBe(true);
  });

  it("treats a subtree as an overlap in both directions", () => {
    expect(pathsOverlap("/etc/ssl", "/etc")).toBe(true);
    expect(pathsOverlap("/run", "/run/systemd/resolve")).toBe(true);
  });

  it("is segment-aware — a shared prefix that is not a subtree does not overlap", () => {
    expect(pathsOverlap("/etcetera", "/etc")).toBe(false);
    expect(pathsOverlap("/srv/models", "/srv/data")).toBe(false);
  });
});

describe("bindPathIssue", () => {
  it("accepts an absolute path outside the baseline and the sensitive set", () => {
    expect(bindPathIssue("/srv/models")).toBeNull();
  });

  it("asks for a path when the row is blank", () => {
    expect(bindPathIssue("")).toBe("Name a host path to mount");
  });

  it("refuses a relative path", () => {
    expect(bindPathIssue("srv/models")).toContain("absolute");
  });

  it("refuses a bare root — under the two-character floor", () => {
    expect(bindPathIssue("/")).toContain(`Between 2 and ${MAX_BIND_PATH_LEN}`);
  });

  it("refuses a path past PATH_MAX", () => {
    expect(bindPathIssue(`/${"a".repeat(MAX_BIND_PATH_LEN)}`)).toContain("Between 2");
  });

  it("refuses a trailing slash so one mount has one spelling", () => {
    expect(bindPathIssue("/srv/models/")).toContain("trailing slash");
  });

  it("refuses an embedded NUL, which would truncate the path", () => {
    expect(bindPathIssue("/srv/mo\0dels")).toContain("NUL");
  });

  it("refuses a .. segment — a bind cannot escape the path it names", () => {
    expect(bindPathIssue("/srv/../root")).toContain("..");
  });

  // `/usr` and `/bin` are here rather than below because the daemon MOUNTS
  // them: the engine's model transport spawns `curl`, so the executable trees
  // are part of the baseline. The refusal an operator sees is "already
  // mounted", not "forbidden", and the distinction is the honest one — adding
  // them changes nothing rather than being dangerous.
  it.each(["/etc", "/etc/ssl", "/run/systemd/resolve", "/usr", "/bin"])(
    "refuses %s because the daemon already mounts that subtree",
    (path) => {
      expect(bindPathIssue(path)).toContain("already mounts");
    },
  );

  it("refuses /run for shadowing a baseline path it contains, not merely for being sensitive", () => {
    // bwrap applies binds in argv order and the last write to a target wins, so
    // an operator entry that CONTAINS /run/systemd/resolve would swallow the
    // resolver mount whole — the M167 incident, spelled the other way round.
    expect(bindPathIssue("/run")).toContain("already mounts /run/systemd/resolve");
  });

  // `/opt` moved HERE from the already-mounted list, and that move is the
  // security change: the daemon writes its control-plane token to
  // `/opt/agentsfleet/.env`, so `/opt` is no longer bound into a lease and an
  // operator naming it is now refused as host control rather than waved off as
  // redundant. Same for the broad `/etc`, which carries the account database.
  it.each(["/opt", "/proc/self", "/root", "/var/lib/agentsfleet", "/var/run"])(
    "refuses %s as the sandbox floor or host control",
    (path) => {
      expect(bindPathIssue(path)).toContain("cannot be bound");
    },
  );
});

describe("bindNoteIssue", () => {
  it("accepts a note at the cap", () => {
    expect(bindNoteIssue("n".repeat(MAX_BIND_NOTE_LEN))).toBeNull();
  });

  it("refuses one character past it", () => {
    expect(bindNoteIssue("n".repeat(MAX_BIND_NOTE_LEN + 1))).toContain(String(MAX_BIND_NOTE_LEN));
  });
});

describe("bindsFromForm", () => {
  it("drops a row the operator opened and left blank rather than refusing the save", () => {
    expect(bindsFromForm([{ ...BIND_ROW_DEFAULT }, { path: "/srv/models", mode: "read_only", note: "" }])).toEqual([
      { path: "/srv/models", mode: "read_only" },
    ]);
  });

  it("trims the path and carries a note through", () => {
    expect(bindsFromForm([{ path: "  /srv/models  ", mode: "read_write", note: "  gpu weights  " }])).toEqual([
      { path: "/srv/models", mode: "read_write", note: "gpu weights" },
    ]);
  });

  it("omits an empty note so the stored assignment carries no empty keys", () => {
    const [bind] = bindsFromForm([{ path: "/srv/models", mode: "read_only", note: "   " }]);
    expect(bind).toEqual({ path: "/srv/models", mode: "read_only" });
    expect("note" in (bind ?? {})).toBe(false);
  });
});

describe("formFromBinds", () => {
  it("reads an absent bind list as no rows", () => {
    expect(formFromBinds(undefined)).toEqual([]);
  });

  it("defaults an entry that names no mode to read-only — access never widens by omission", () => {
    expect(formFromBinds([{ path: "/srv/models" }])).toEqual([
      { path: "/srv/models", mode: "read_only", note: "" },
    ]);
  });

  it("carries an explicit mode and note through unchanged", () => {
    expect(formFromBinds([{ path: "/srv/cache", mode: "read_write", note: "build cache" }])).toEqual([
      { path: "/srv/cache", mode: "read_write", note: "build cache" },
    ]);
  });
});
