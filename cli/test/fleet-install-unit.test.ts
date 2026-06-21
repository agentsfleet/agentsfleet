// Unit-style coverage for src/commands/fleet_install.ts branches that are
// unreachable via the Command-Line Interface (CLI) integration path.
//
// Two branches need Effect-layer injection:
//   Lines 143-148: updateEffectFromArgs with undefined fleetId. Commander
//     enforces <fleet_id> as a required positional so runCli never reaches
//     the guard — the handler must be called directly.
//   Lines 55-56: loadBundle catch block — generic Error (not SkillLoadError).
//     loadSkillFromPath only throws SkillLoadError so this branch requires a
//     patched loadSkillFromPath substitute that throws a plain Error.
//
// Pattern mirrors fleet-steer.integration.test.ts unit-style section.

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

// ── Lines 143-148: updateEffectFromArgs with undefined fleetId ──────────────

describe("updateEffectFromArgs — undefined fleetId (lines 143-148)", () => {
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

// ── loadBundle defensive catch ladder ────────────────────────────────────────
//
// loadSkillFromPath only throws SkillLoadError today (that arm is covered by
// fleet-install.integration.test.ts via a real bad path). The other two arms
// guard against a future foreign throw — reachable here because loadBundle
// takes an injectable `loader`. The rule being pinned: a foreign error
// must still render a readable detail, never `undefined: ...`.

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

describe("installEffectFromFlags — SKILL.md-only bundle", () => {
  test("omits trigger_markdown and prints the generated-trigger note", async () => {
    const dir = await makeSkillOnlyDir();
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    try {
      const exit = await Effect.runPromiseExit(
        installEffectFromFlags({ fromPath: dir }).pipe(
          Effect.provide(makeLayer(captured, false, requests, {
            fleet_id: "01900000-0000-7000-8000-0000000a91d1",
            name: "skill-only-install",
            webhook_urls: {},
          })),
        ),
      );
      expect(Exit.isSuccess(exit)).toBe(true);
      expect(requests[0]?.body).toEqual({ source_markdown: SKILL_ONLY_MD });
      expect(captured.join("\n")).toContain("Generated default API wake");
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});

describe("installEffectFromFlags — no source", () => {
  test("empty fromPath string produces ValidationError naming both sources", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ fromPath: "" }).pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("a source is required");
    }
  });

  test("empty flags object produces ValidationError naming both sources", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({}).pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("--from <path> or --template <id>");
    }
  });
});

const TEMPLATE_BUNDLE_ID = "01900000-0000-7000-8000-0000000bund1";
const TEMPLATE_FLEET_ID = "01900000-0000-7000-8000-0000000f1ee7";

describe("installEffectFromFlags — template JSON mode", () => {
  test("prints structured install output in JSON mode", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ templateId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, true, requests, {
          bundle_id: TEMPLATE_BUNDLE_ID,
          fleet_id: TEMPLATE_FLEET_ID,
          name: "t1",
          webhook_urls: {},
          requirements: { trigger_present: true },
        })),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.join("\n")).toContain("\"status\":\"installed\"");
  });
});

describe("installEffectFromFlags — webhook URLs", () => {
  test("prints webhook URLs when the create response carries them", async () => {
    const dir = await makeSkillOnlyDir();
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    try {
      const exit = await Effect.runPromiseExit(
        installEffectFromFlags({ fromPath: dir }).pipe(
          Effect.provide(makeLayer(captured, false, requests, {
            fleet_id: "01900000-0000-7000-8000-0000000a91d1",
            name: "skill-only-install",
            webhook_urls: { github: "https://api.example/webhooks/github" },
          })),
        ),
      );
      expect(Exit.isSuccess(exit)).toBe(true);
      const out = captured.join("\n");
      expect(out).toContain("Webhook URLs");
      expect(out).toContain("github: https://api.example/webhooks/github");
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});

describe("installEffectFromFlags — template requirements preview", () => {
  test("prints credentials, tools and network hosts, plus the generated-trigger note", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ templateId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests, {
          bundle_id: TEMPLATE_BUNDLE_ID,
          fleet_id: TEMPLATE_FLEET_ID,
          name: "t1",
          webhook_urls: {},
          requirements: {
            credentials: ["github"],
            tools: ["github_review_comment"],
            network_hosts: ["api.github.com"],
            trigger_present: false,
          },
        })),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const out = captured.join("\n");
    expect(out).toContain("Credentials: github");
    expect(out).toContain("Tools: github_review_comment");
    expect(out).toContain("Network hosts: api.github.com");
    expect(out).toContain("Generated default API wake");
  });

  test("tolerates a snapshot with no requirements field", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ templateId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests, {
          bundle_id: TEMPLATE_BUNDLE_ID,
          fleet_id: TEMPLATE_FLEET_ID,
          name: "t1",
          webhook_urls: {},
        })),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.join("\n")).toContain("t1 is live.");
  });
});

describe("installEffectFromFlags — snapshot without bundle_id", () => {
  test("fails with a ConfigError when the import returns no bundle_id", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ templateId: "t1" }).pipe(
        Effect.provide(makeLayer(captured, false, requests, {
          requirements: { trigger_present: true },
        })),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("did not return a bundle_id");
    }
  });
});

describe("installEffectFromFlags — both sources", () => {
  test("--from and --template together are rejected as mutually exclusive", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ fromPath: "/x", templateId: "t" }).pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("mutually exclusive");
    }
  });
});

describe("installEffectFromFlags — template source", () => {
  test("imports a snapshot then creates with bundle_id + name override", async () => {
    const captured: string[] = [];
    const requests: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags({ templateId: "github-pr-reviewer", name: "pr-reviewer-frontend" }).pipe(
        Effect.provide(
          makeLayer(captured, false, requests, {
            // snapshot import + fleet create both return this stub
            bundle_id: "01900000-0000-7000-8000-0000000bun01",
            fleet_id: "01900000-0000-7000-8000-0000000f1ee7",
            name: "pr-reviewer-frontend",
            webhook_urls: {},
            requirements: { credentials: ["github"], trigger_present: true },
          }),
        ),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // first request imports the template snapshot
    expect(requests[0]?.body).toEqual({
      source_kind: "template",
      source_ref: "github-pr-reviewer",
    });
    // second request creates the fleet from the snapshot, carrying the override
    expect(requests[1]?.body).toEqual({
      bundle_id: "01900000-0000-7000-8000-0000000bun01",
      name: "pr-reviewer-frontend",
    });
  });
});
