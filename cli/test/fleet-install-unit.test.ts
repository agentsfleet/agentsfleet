// Unit-style coverage for src/commands/fleet_install.ts branches.
//
// `install` is template-only: it resolves the workspace gallery (GET) then
// creates the fleet (POST) keyed off the entry's tier. The shared mock returns
// one response for both calls, so the template fixtures carry both `items`
// (gallery) and the create fields (`fleet_id`/`name`/`webhook_urls`). Local
// `--from` install was removed with the two-tier model; the live-edit
// `fleet update --from` PATCH path (loadBundle/bodyFromBundle) is unchanged and
// covered here via updateEffectFromArgs + loadBundle.

import { describe, test, expect } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import {
  updateEffectFromArgs,
  installEffectFromFlags,
  loadBundle,
} from "../src/commands/fleet_install.ts";
import { loadSkillFromPath } from "../src/lib/load-skill-from-path.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";

const WS_ID = "01900000-0000-7000-8000-000000c1a172";
const TOKEN = "test.jwt.unit";
const SKILL_ONLY_MD =
  "---\nname: skill-only-install\ndescription: Installs without trigger\nversion: 0.1.0\n---\n# Skill only\n";

const makeLayer = (
  captured: string[],
  jsonMode = false,
  requests: HttpRequestInput[] = [],
  response: unknown = {},
) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: "https://api.unit-test.local",
      dashboardUrl: "https://dash.unit-test.local",
      accessToken: Option.none(),
      jsonMode,
      noOpen: false,
      telemetryPosthogKey: "phc_unit",
      telemetryPosthogHost: "https://us.i.posthog.com",
    }),
    Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.some(Redacted.make(TOKEN))),
      getSavedAt: Effect.sync(() => null),
      getSessionId: Effect.sync(() => null),
      getApiUrl: Effect.sync(() => null),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(Workspaces, {
      load: Effect.sync(() => ({
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "unit-ws", created_at: Date.now() }],
      })),
      save: () => Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) =>
        Effect.sync(() => {
          requests.push(input);
          return response as T;
        }),
    }),
    Layer.succeed(Output, {
      intro: (m) => Effect.sync(() => { captured.push(m); }),
      info: (m) => Effect.sync(() => { captured.push(m); }),
      success: (m) => Effect.sync(() => { captured.push(m); }),
      warn: (m) => Effect.sync(() => { captured.push(m); }),
      error: (m) => Effect.sync(() => { captured.push(m); }),
      outro: (m) => Effect.sync(() => { captured.push(m); }),
      printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: () => Effect.void,
    }),
  );

async function makeSkillOnlyDir(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "agentsfleet-skill-only-"));
  await writeFile(join(dir, "SKILL.md"), SKILL_ONLY_MD);
  return dir;
}

const LIBRARY_FLEET_ID = "01900000-0000-7000-8000-0000000f1ee7";

// A gallery+create stub for a `--library` run: `items` answers the gallery GET,
// the create fields answer the POST (the mock returns the same object for both).
const libraryResponse = (
  id: string,
  visibility: string,
  requirements: Record<string, unknown> | undefined,
  createName: string,
  webhookUrls: Record<string, string> = {},
) => ({
  items: [{ id, name: createName, visibility, ...(requirements ? { requirements } : {}) }],
  fleet_id: LIBRARY_FLEET_ID,
  name: createName,
  webhook_urls: webhookUrls,
});

// ── updateEffectFromArgs with undefined fleetId (live-edit path) ─────────────

describe("updateEffectFromArgs — undefined fleetId", () => {
  test("undefined fleetId produces ValidationError on the error channel", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      updateEffectFromArgs(undefined, "/any/path").pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("fleet_id is required");
    }
  });
});

// ── loadBundle defensive catch ladder (still used by `fleet update --from`) ──
//
// loadSkillFromPath only throws SkillLoadError today; the other arms guard
// against a future foreign throw — reachable here because loadBundle takes an
// injectable `loader`. The rule pinned: a foreign error must still render a
// readable detail, never `undefined: ...`.

describe("loadBundle — foreign error catch arm", () => {
  test("a non-SkillLoadError throw surfaces a readable detail, never 'undefined:'", async () => {
    const err = await Effect.runPromise(
      loadBundle("/any/path", () => { throw new TypeError("boom from loader"); }).pipe(
        Effect.flip,
      ),
    );
    expect(err.message).toContain("boom from loader");
    expect(err.message).not.toContain("undefined");
  });
});

describe("loadSkillFromPath — optional TRIGGER.md", () => {
  test("a directory with only SKILL.md loads with a null trigger", async () => {
    const dir = await makeSkillOnlyDir();
    try {
      const bundle = loadSkillFromPath(dir);
      expect(bundle.skill_md).toBe(SKILL_ONLY_MD);
      expect(bundle.trigger_md).toBeNull();
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});

describe("installEffectFromFlags — missing --library", () => {
  test("no library id produces a ValidationError requiring --library", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({}).pipe(
        Effect.provide(makeLayer(captured, false, requests)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("--library <id> is required");
    }
    // The required-flag guard fires before any API call.
    expect(requests.length).toBe(0);
  });
});

describe("installEffectFromFlags — template JSON mode", () => {
  test("prints structured install output in JSON mode", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, true, requests,
          libraryResponse("t1", "platform", { trigger_present: true }, "t1"))),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.join("\n")).toContain("\"status\":\"installed\"");
  });
});

describe("installEffectFromFlags — template webhook URLs", () => {
  test("prints webhook URLs when the create response carries them", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests,
          libraryResponse("t1", "platform", { trigger_present: true }, "t1",
            { github: "https://api.example/webhooks/github" }))),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const out = captured.join("\n");
    expect(out).toContain("Webhook URLs");
    expect(out).toContain("github: https://api.example/webhooks/github");
  });
});

describe("installEffectFromFlags — template requirements preview", () => {
  test("prints credentials, tools and network hosts, plus the generated-trigger note", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests,
          libraryResponse("t1", "platform", {
            credentials: ["github"],
            tools: ["github_review_comment"],
            network_hosts: ["api.github.com"],
            trigger_present: false,
          }, "t1"))),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const out = captured.join("\n");
    expect(out).toContain("Credentials: github");
    expect(out).toContain("Tools: github_review_comment");
    expect(out).toContain("Network hosts: api.github.com");
    expect(out).toContain("Generated default API wake");
  });

  test("tolerates a gallery entry with no requirements field", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests,
          libraryResponse("t1", "platform", undefined, "t1"))),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.join("\n")).toContain("t1 is live.");
  });
});

describe("installEffectFromFlags — template not in the gallery", () => {
  test("fails with a ConfigError when the id is absent from the workspace gallery", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests, { items: [] })),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("is not in this workspace's gallery");
    }
  });
});

describe("installEffectFromFlags — unrecognized template tier", () => {
  test("fails with a ConfigError when the gallery entry visibility is neither tier", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests,
          libraryResponse("t1", "weird", { trigger_present: true }, "t1"))),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("unrecognized tier");
    }
  });
});

describe("installEffectFromFlags — resolves the gallery then installs by tier", () => {
  test("GETs the gallery, then creates with platform_template_id + name override", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: "github-pr-reviewer", name: "pr-reviewer-frontend" }).pipe(
        Effect.provide(
          makeLayer(captured, false, requests,
            libraryResponse("github-pr-reviewer", "platform",
              { credentials: ["github"], trigger_present: true }, "pr-reviewer-frontend")),
        ),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // first request resolves the workspace gallery (GET)
    expect(requests[0]?.method).toBe("GET");
    expect(requests[0]?.path).toContain("/fleet-libraries");
    // second request creates the fleet keyed off the platform tier, with override
    expect(requests[1]?.body).toEqual({
      platform_library_id: "github-pr-reviewer",
      name: "pr-reviewer-frontend",
    });
  });

  test("installs a tenant-tier entry by tenant_template_id", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const tenantId = "0195b4ba-8d3a-7f13-8abc-0000000000a1";
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ libraryId: tenantId }).pipe(
        Effect.provide(
          makeLayer(captured, false, requests,
            libraryResponse(tenantId, "tenant", { trigger_present: true }, "my-tenant-fleet")),
        ),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(requests[1]?.body).toEqual({ tenant_library_id: tenantId });
  });
});
