// Shared fixtures for the `library add` integration suites.
//
// Extracted when library-add.integration.test.ts passed the repository's
// 350-line cap.

import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { withAuthedStateDir } from "./helpers-cli-state.ts";

export const WS_ID = "01900000-0000-7000-8000-00000067e210";
export const LIBRARIES = `/v1/workspaces/${WS_ID}/fleet-libraries`;
export const LIBRARY_ID = "01900000-0000-7000-8000-0000000aa001";

export const SKILL_MD = "---\nname: probe\n---\n# Probe\n";
export const TRIGGER_MD = "---\nname: probe\n---\n# Wake rule\n";

export const created = (overrides: Record<string, unknown> = {}) => ({
  id: LIBRARY_ID,
  name: "probe",
  visibility: "tenant",
  requirements: { credentials: ["github"], tools: [], network_hosts: [], trigger_present: true },
  ...overrides,
});

export const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_library_add" }, fn);

/** A bundle directory on disk; `withTrigger: false` omits TRIGGER.md, which the
 *  daemon treats as optional. */
export const withBundle = async <T>(
  withTrigger: boolean,
  fn: (dir: string) => Promise<T>,
): Promise<T> => {
  const dir = mkdtempSync(join(tmpdir(), "af-bundle-"));
  try {
    writeFileSync(join(dir, "SKILL.md"), SKILL_MD);
    if (withTrigger) writeFileSync(join(dir, "TRIGGER.md"), TRIGGER_MD);
    return await fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

export const parseBody = (raw: string | null): Record<string, unknown> =>
  raw === null ? {} : (JSON.parse(raw) as Record<string, unknown>);
