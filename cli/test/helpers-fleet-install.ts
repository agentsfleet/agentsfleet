// The fixtures both fleet-install suites share: the authenticated scope, the
// bundle directory a source install reads, and the gallery route a library
// install resolves through.
//
// Extracted when `fleet-install.integration.test.ts` crossed the 350-line file
// cap and split by verb — install here, update beside it. Two copies of this
// header would drift the moment one suite changed a fixture.

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

export const WS_ID = "01900000-0000-7000-8000-000000c1a170";
export const FLEET_ID = "01900000-0000-7000-8000-000000c1a171";

// The gallery entry a `--library` install resolves.
export const TEMPLATE_ID = "github-pr-reviewer";

export const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_install" }, fn);

export async function makeBundleDir(name: string): Promise<string> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), `zctl-${name}-`));
  await fs.writeFile(path.join(dir, "SKILL.md"),
    `---\nname: ${name}\n---\n# ${name}\n`, { mode: 0o644 });
  await fs.writeFile(path.join(dir, "TRIGGER.md"),
    `---\nname: ${name}\n---\n# trigger\n`, { mode: 0o644 });
  return dir;
}

// A gallery (GET) route returning one platform-tier entry whose id matches,
// paired with the create (POST) route. A `--library` install makes both calls:
// resolve the gallery, then create the fleet keyed off the entry's tier.
export function galleryRoute(
  id: string,
  name: string | undefined,
  requirements: Record<string, unknown> = { trigger_present: true },
): MockRoutes {
  return {
    [`GET /v1/workspaces/${WS_ID}/fleet-libraries`]: () =>
      jsonResponse(200, {
        items: [{ id, ...(name ? { name } : {}), visibility: "platform", requirements }],
      }),
  };
}

export { withMockApi, jsonResponse, type MockRoutes };
