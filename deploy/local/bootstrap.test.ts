import { afterEach, beforeEach, describe, expect, mock, spyOn, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { chmod, readFile, rm, stat, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { z } from "zod";
import { configureConnector, configureDecisions, configureScheduler, configureSigning, enroll, localTarget, persistWorkspace, privateFile } from "./bootstrap";

type BootstrapRequest = Parameters<typeof enroll>[0];
type Handler = (method: string, path: string, body?: unknown) => unknown;

const ROOT = resolve(import.meta.dir, "../..");
const SHELL_TIMEOUT_MS = 30_000;
// Stubs only what reaches Docker: `dev_compose` answers the database-exists query.
const SCRIPT = [
  "set -euo pipefail",
  'source "$1/deploy/local/bootstrap.sh"',
  "dev_fail() { printf '%s\\n' \"$*\" >&2; exit 1; }",
  "dev_infrastructure() { :; }",
  "dev_compose() { printf '%s' \"${EXISTING_DATABASE:-f}\"; }",
  "acceptance_environment",
  "",
].join("\n");
const LOCAL_API = "http://127.0.0.1";
const FIXTURE_WORKSPACE = "local-fixture";
const SECRETS_PATH = `/v1/workspaces/${FIXTURE_WORKSPACE}/secrets`;
const SECRET_BAG = z.strictObject({ name: z.string(), data: z.record(z.string(), z.string()) });
const GITHUB_FIELDS = ["app_id", "app_slug", "client_id", "client_secret", "private_key_pem", "webhook_secret"];
const GITHUB_NAMES = GITHUB_FIELDS.map((field) => `LOCAL_GITHUB_${field.toUpperCase()}`);
const CONNECTOR_ENV = [...GITHUB_NAMES, "ACCEPTANCE_LOCAL_CONNECTOR_VAULT", "TYPESAFE_API_KEY"];
const DECISION_ID = "01900000-0000-7000-8000-0000000000d1";
const MODELS_PATH = "/v1/admin/models";

let root = "";
let savedEnv: Record<string, string | undefined> = {};

/** The bootstrap's request is generic over its reply; each fake answers with the shape its route returns. */
function fakeRequest(handler: Handler): BootstrapRequest {
  return async <T>(method: string, path: string, body?: unknown): Promise<T> => (await handler(method, path, body)) as T;
}

/** `typeof fetch` also carries `preconnect`, which no code under test calls. */
function fakeFetch(handler: (url: string | URL | Request, init?: RequestInit) => Promise<Response>): typeof fetch {
  return Object.assign(handler, { preconnect: globalThis.fetch.preconnect });
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "acceptance-bootstrap-"));
  savedEnv = Object.fromEntries(CONNECTOR_ENV.map((name) => [name, process.env[name]]));
});

afterEach(async () => {
  mock.restore();
  for (const [name, value] of Object.entries(savedEnv)) {
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
  }
  await rm(root, { recursive: true, force: true });
});

describe("acceptance environment file", () => {
  let file = "";
  let env: Record<string, string | undefined> = {};

  beforeEach(() => {
    file = join(root, "acceptance.env");
    env = { ...process.env, DEV_STATE: root, DEV_ID: "wt_1234abcd", LOCAL_DEV_ENV_FILE: file, DEV_DB: "dev_wt_1234abcd" };
    delete env.AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE;
    delete env.ENCRYPTION_MASTER_KEY;
  });

  function runBootstrap(): { exitCode: number; stdout: string; stderr: string } {
    const result = Bun.spawnSync(["bash", "-c", SCRIPT, "--", ROOT], {
      env, stdin: "ignore", stdout: "pipe", stderr: "pipe", timeout: SHELL_TIMEOUT_MS,
    });
    return { exitCode: result.exitCode, stdout: result.stdout.toString(), stderr: result.stderr.toString() };
  }

  test("creates private configuration and reuses encryption key", () => {
    const first = runBootstrap();
    expect(first.exitCode, first.stderr).toBe(0);
    const content = readFileSync(file, "utf8");
    expect(content).toMatch(/ENCRYPTION_MASTER_KEY=[a-f0-9]{64}\n/);
    expect(content).toContain(`ACCEPTANCE_LOCAL_RUNNER_RECORD=${root}/acceptance/runner.json`);
    expect(statSync(file).mode & 0o777).toBe(0o600);
    expect(first.stdout).toBe("");
    expect(runBootstrap().exitCode).toBe(0);
    expect(readFileSync(file, "utf8")).toBe(content);
  });

  test("explicit missing file is not silently replaced", () => {
    env.AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE = file;
    expect(runBootstrap().exitCode).not.toBe(0);
    expect(existsSync(file)).toBe(false);
  });

  test("existing signed toolbox directory can be selected", () => {
    env.ACCEPTANCE_LOCAL_TOOLBOX_DIR = join(root, "verified toolbox");
    const result = runBootstrap();
    expect(result.exitCode, result.stderr).toBe(0);
    expect(readFileSync(file, "utf8")).toContain("verified\\ toolbox");
  });

  test("symlink cannot replace another environment", () => {
    const other = join(root, "other.env");
    writeFileSync(other, "keep this file\n");
    symlinkSync(other, file);
    expect(runBootstrap().exitCode).not.toBe(0);
    expect(readFileSync(other, "utf8")).toBe("keep this file\n");
  });

  test("interrupted creation does not overwrite its pending key", () => {
    const pending = join(root, "acceptance.env.new");
    writeFileSync(pending, "unfinished\n");
    expect(runBootstrap().exitCode).not.toBe(0);
    expect(readFileSync(pending, "utf8")).toBe("unfinished\n");
    expect(existsSync(file)).toBe(false);
  });

  test("existing configuration is kept without logging its contents", () => {
    writeFileSync(file, "ENCRYPTION_MASTER_KEY=preserve-private-value\n");
    chmodSync(file, 0o600);
    const result = runBootstrap();
    expect(result.exitCode, result.stderr).toBe(0);
    expect(readFileSync(file, "utf8")).toBe("ENCRYPTION_MASTER_KEY=preserve-private-value\n");
    expect(result.stdout + result.stderr).not.toContain("preserve-private-value");
  });

  test("existing database never gets a new encryption key", () => {
    env.EXISTING_DATABASE = "t";
    expect(runBootstrap().exitCode).not.toBe(0);
    expect(existsSync(file)).toBe(false);
    env.ENCRYPTION_MASTER_KEY = "original-private-value";
    const result = runBootstrap();
    expect(result.exitCode, result.stderr).toBe(0);
    expect(readFileSync(file, "utf8")).toContain("ENCRYPTION_MASTER_KEY=original-private-value");
    expect(result.stdout + result.stderr).not.toContain("original-private-value");
  });
});

describe("local target", () => {
  test("remote bootstrap targets are refused before provisioning", () => {
    const local = "http://127.0.0.1:30999";
    expect(localTarget(local, local)).toBe(local);
    for (const target of ["https://api-dev.agentsfleet.net", "http://127.0.0.1:31000",
      `${local}/v1`, `${local}?remote=yes`, local.replace("127.0.0.1", "secret@127.0.0.1"), `${local}#fragment`]) {
      expect(() => localTarget(target, local)).toThrow(/loopback API/);
    }
    expect(() => localTarget("https://api-dev.agentsfleet.net", "https://api-dev.agentsfleet.net")).toThrow(/loopback API/);
  });
});

describe("runner enrollment", () => {
  test("runner records must be private owned regular files", async () => {
    const file = `${root}/runner.json`;
    const link = `${file}.link`;
    await writeFile(file, "{}", { mode: 0o600 });
    await privateFile(file);
    await symlink(file, link);
    await expect(privateFile(link)).rejects.toThrow(/private, owned/);
    await chmod(file, 0o644);
    await expect(privateFile(file)).rejects.toThrow(/private, owned/);
    await expect(privateFile(root)).rejects.toThrow(/private, owned/);
  });

  test("runner response loss and older enrollment never create duplicates", async () => {
    const file = `${root}/runner.json`;
    const namespace = "wt_1234abcd";
    let writes = 0;
    await expect(enroll(fakeRequest((method) => {
      if (method === "GET") return { items: [], next_cursor: null };
      writes++;
      throw new Error("reply lost");
    }), LOCAL_API, namespace, file, DECISION_ID)).rejects.toThrow(/reply lost/);
    expect(await Bun.file(file).exists()).toBe(false);
    await expect(enroll(fakeRequest(() => expect.unreachable("ambiguous enrollment must not repeat")),
      LOCAL_API, namespace, file, DECISION_ID)).rejects.toThrow(/unfinished/);
    expect(writes).toBe(1);
    const pages: string[] = [];
    await expect(enroll(fakeRequest((method, path) => {
      expect(method).toBe("GET");
      pages.push(path);
      return pages.length === 1 ? { items: [{ host_id: "another-host" }], next_cursor: "older/page" }
        : { items: [{ host_id: `local-acceptance-${namespace}` }], next_cursor: null };
    }), LOCAL_API, namespace, `${file}.orphan`, DECISION_ID)).rejects.toThrow(/already enrolled/);
    expect(pages).toEqual(["/v1/admin/runners", "/v1/admin/runners?starting_after=older%2Fpage"]);
    expect(await Bun.file(`${file}.orphan`).exists()).toBe(false);
  });

  test("untrusted or repeated runner pages refuse before enrollment", async () => {
    const file = `${root}/runner.json`;
    for (const page of [{ items: [] }, { items: [{ host_id: null }], next_cursor: null }, { items: [], next_cursor: "" }]) {
      const attempt = enroll(fakeRequest((method) => {
        expect(method).toBe("GET");
        return page;
      }), LOCAL_API, "wt_test", file, DECISION_ID);
      await expect(attempt).rejects.toBeInstanceOf(Error);
      await expect(attempt).rejects.toMatchObject({ name: "ZodError" });
      expect(await Bun.file(`${file}.enrolling`).exists()).toBe(false);
      expect(await Bun.file(file).exists()).toBe(false);
    }
    await expect(enroll(fakeRequest((method) => {
      expect(method).toBe("GET");
      return { items: [], next_cursor: "repeated" };
    }), LOCAL_API, "wt_test", file, DECISION_ID)).rejects.toThrow(/repeated a cursor/);
    expect(await Bun.file(`${file}.enrolling`).exists()).toBe(false);
    expect(await Bun.file(file).exists()).toBe(false);
  });

  test("runner record is atomic private and reused", async () => {
    const file = `${root}/runner.json`;
    const registration = { runner_id: "01900000-0000-7000-8000-000000000001", runner_token: "agt_r_test_only" };
    let writes = 0;
    const request = fakeRequest(async (method, _path, body) => {
      expect(await Bun.file(file).exists()).toBe(false);
      if (method === "GET") return { items: [], next_cursor: null };
      expect(await Bun.file(`${file}.enrolling`).exists()).toBe(true);
      // The setup edit check runs only for a runner that names a decision model.
      expect(body).toMatchObject({ assigned_policy: { decision_model_id: DECISION_ID } });
      writes++;
      return registration;
    });
    await enroll(request, LOCAL_API, "wt_1234abcd", file, DECISION_ID);
    expect(await Bun.file(file).json()).toEqual(registration);
    expect((await stat(file)).mode & 0o777).toBe(0o600);
    expect(await Bun.file(`${file}.enrolling`).exists()).toBe(false);
    expect(await Bun.file(`${file}.new`).exists()).toBe(false);
    const verify = spyOn(globalThis, "fetch").mockImplementation(fakeFetch(async (url) => {
      expect(url).toBe(`${LOCAL_API}/v1/runners/me`);
      return Response.json({});
    }));
    await enroll(fakeRequest(() => expect.unreachable("existing runner must not enroll again")), LOCAL_API, "wt_1234abcd", file, DECISION_ID);
    expect(verify).toHaveBeenCalledTimes(1);
    expect(writes).toBe(1);
  });
});

describe("fixture secret bags", () => {
  test("local signing is created once and kept after lost acknowledgement", async () => {
    let bag: z.infer<typeof SECRET_BAG> | undefined;
    let writes = 0;
    let loseAcknowledgement = true;
    const request = fakeRequest((method, url, body) => {
      expect(url).toBe(SECRETS_PATH);
      if (method === "GET") return { secrets: bag ? [{ name: bag.name }] : [] };
      expect(method).toBe("POST");
      // Plain matchers: Bun's toMatchObject writes asymmetric matchers into the received object.
      const posted = SECRET_BAG.parse(structuredClone(body));
      expect(posted.name).toBe("approval-signing");
      expect(posted.data.webhook_secret).toMatch(/^[a-f0-9]{64}$/);
      bag = posted;
      writes++;
      if (loseAcknowledgement) {
        loseAcknowledgement = false;
        throw new Error("reply lost");
      }
      return {};
    });
    await configureSigning(fakeRequest(() => expect.unreachable("must not access another platform workspace")), FIXTURE_WORKSPACE, "other");
    await expect(configureSigning(request, FIXTURE_WORKSPACE, FIXTURE_WORKSPACE)).rejects.toThrow(/reply lost/);
    const original = structuredClone(bag);
    await configureSigning(request, FIXTURE_WORKSPACE, FIXTURE_WORKSPACE);
    await configureSigning(request, FIXTURE_WORKSPACE);
    expect(writes).toBe(1);
    expect(bag).toEqual(original);
    bag = undefined;
    await configureSigning(request, FIXTURE_WORKSPACE);
    expect(writes).toBe(2);
  });

  test("local scheduler bag is read from the emulator and kept once", async () => {
    const token = Buffer.from('{"UserID":"defaultUser","Password":"defaultPassword"}').toString("base64");
    let bag: z.infer<typeof SECRET_BAG> | undefined;
    let writes = 0;
    let keyReads = 0;
    const emulator = spyOn(globalThis, "fetch").mockImplementation(fakeFetch(async (url, init) => {
      expect(url).toBe("http://127.0.0.1:18080/v2/keys");
      expect(new Headers(init?.headers).get("Authorization")).toBe(`Bearer ${token}`);
      keyReads++;
      return Response.json({ current: "sig_current", next: "sig_next" });
    }));
    const request = fakeRequest((method, url, body) => {
      expect(url).toBe(SECRETS_PATH);
      if (method === "GET") return { secrets: bag ? [{ name: bag.name }] : [] };
      expect(method).toBe("POST");
      bag = SECRET_BAG.parse(structuredClone(body));
      writes++;
      return {};
    });
    await configureScheduler(fakeRequest(() => expect.unreachable("must not access another platform workspace")), FIXTURE_WORKSPACE, "18080", "other");
    expect(keyReads).toBe(0);
    await configureScheduler(request, FIXTURE_WORKSPACE, "18080", FIXTURE_WORKSPACE);
    expect(bag).toEqual({ name: "qstash", data: { token, url: "http://127.0.0.1:18080/v2",
      current_signing_key: "sig_current", next_signing_key: "sig_next" } });
    await configureScheduler(request, FIXTURE_WORKSPACE, "18080");
    expect(writes).toBe(1);
    const noWrite = fakeRequest(() => expect.unreachable("a failed read must not reach the secrets API"));
    const silent = /QStash emulator did not answer/;
    emulator.mockImplementation(fakeFetch(async () => new Response("down", { status: 503 })));
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "18080")).rejects.toThrow(silent);
    emulator.mockImplementation(fakeFetch(async () => { throw new TypeError("connection refused"); }));
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "18080")).rejects.toThrow(silent);
    emulator.mockImplementation(fakeFetch(async () => Response.json({ current: "sig_current", next: "" })));
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "18080")).rejects.toMatchObject({ name: "ZodError" });
    emulator.mockImplementation(fakeFetch(async () => Response.json({ current: "sig_current" })));
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "18080")).rejects.toMatchObject({ name: "ZodError" });
    emulator.mockImplementation(fakeFetch(async () => expect.unreachable("a malformed port must not reach the emulator")));
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "18o80")).rejects.toMatchObject({ name: "ZodError" });
    await expect(configureScheduler(noWrite, FIXTURE_WORKSPACE, "")).rejects.toMatchObject({ name: "ZodError" });
  });

  test("connector fields come from the caller or one op read each", async () => {
    let saved: unknown;
    let calls: string[][] = [];
    const request = fakeRequest((method, url, body) => {
      expect(url).toBe(SECRETS_PATH);
      if (method === "GET") return { secrets: [] };
      saved = body;
      return {};
    });
    const op = async (args: string[]): Promise<string> => {
      calls.push(args);
      if (args[0] === "item") return JSON.stringify([
        { title: "github-app", vault: { id: "vault-dev", name: "ZMB_CD_DEV" } },
        { title: "github-app", vault: { id: "vault-prod", name: "ZMB_CD_PROD" } },
      ]);
      return `from-op:${args[2]}`;
    };
    const run = async (): Promise<void> => {
      calls = [];
      saved = undefined;
      await configureConnector(request, FIXTURE_WORKSPACE, op);
    };

    // A burst machine passes every field in: no 1Password at all.
    for (const name of GITHUB_NAMES) process.env[name] = `given-${name}`;
    await run();
    expect(calls).toEqual([]);
    expect(saved).toEqual({ name: "github-app",
      data: Object.fromEntries(GITHUB_FIELDS.map((field, index) => [field, `given-${GITHUB_NAMES[index]}`])) });

    // One field missing: all six come from 1Password, never a mix.
    delete process.env.LOCAL_GITHUB_WEBHOOK_SECRET;
    delete process.env.ACCEPTANCE_LOCAL_CONNECTOR_VAULT;
    await run();
    expect(calls[0]).toEqual(["item", "list", "--format", "json"]);
    expect(calls.slice(1)).toEqual(GITHUB_FIELDS.map((field) => ["read", "--no-newline", `op://vault-dev/github-app/${field}`]));
    expect(saved).toEqual({ name: "github-app",
      data: Object.fromEntries(GITHUB_FIELDS.map((field) => [field, `from-op:op://vault-dev/github-app/${field}`])) });

    // A named vault skips discovery.
    for (const name of GITHUB_NAMES) delete process.env[name];
    process.env.ACCEPTANCE_LOCAL_CONNECTOR_VAULT = "chosen";
    await run();
    expect(calls).toHaveLength(GITHUB_FIELDS.length);
    expect(calls.every((args) => args[0] === "read" && args[2]?.startsWith("op://chosen/github-app/"))).toBe(true);
  });
});

describe("platform workspace record", () => {
  test("configured platform workspace and encryption key are preserved", async () => {
    const file = `${root}/acceptance.env`;
    const original = "ENCRYPTION_MASTER_KEY=keep-existing-key\nPLATFORM_ADMIN_WORKSPACE_ID=existing-workspace\n";
    await writeFile(file, original, { mode: 0o600 });
    await expect(persistWorkspace(file, "different-workspace")).rejects.toThrow(/refusing to replace/);
    expect(await readFile(file, "utf8")).toBe(original);
    await persistWorkspace(file, "existing-workspace");
    expect(await readFile(file, "utf8")).toBe(original);
    await writeFile(file, "ENCRYPTION_MASTER_KEY=keep-existing-key\n");
    await persistWorkspace(file, "new-workspace");
    expect(await readFile(file, "utf8")).toBe("ENCRYPTION_MASTER_KEY=keep-existing-key\nPLATFORM_ADMIN_WORKSPACE_ID=new-workspace\n");
  });
});

describe("setup edit check", () => {
  const modelRow = { id: DECISION_ID, provider: "typesafe", model_id: "jev-1.14.0" };

  test("the decision model is created once and its key saved once in the platform workspace", async () => {
    process.env.TYPESAFE_API_KEY = "ts_test_only";
    let created: unknown;
    let saved: unknown;
    const first = fakeRequest((method, path, body) => {
      if (method === "GET" && path === MODELS_PATH) return { models: [], next_cursor: null };
      if (method === "POST" && path === MODELS_PATH) { created = body; return { ...modelRow, request_id: "req" }; }
      if (method === "GET" && path === SECRETS_PATH) return { secrets: [] };
      if (method === "POST" && path === SECRETS_PATH) { saved = SECRET_BAG.parse(body); return {}; }
      return expect.unreachable(`unexpected ${method} ${path}`);
    });
    expect(await configureDecisions(first, FIXTURE_WORKSPACE)).toBe(DECISION_ID);
    expect(created).toMatchObject({ provider: "typesafe", model_id: "jev-1.14.0", prices: [],
      decision: { enabled: true, profile: { api: "typesafe", endpoint: "https://api.typesafe.ai/v1/systemone",
        accepted_models: ["jev-1.14.0"], capabilities: { noul: true } } } });
    const decision = z.object({ decision: z.object({
      profile: z.object({ evaluation_digest: z.string().regex(/^[0-9a-f]{64}$/) }),
      prices: z.array(z.object({ prompt_tokens_above: z.literal(0), output_nanos_per_mtok: z.literal("0") })).length(1),
    }) }).parse(created);
    expect(decision.decision.prices).toHaveLength(1);
    expect(saved).toEqual({ name: "typesafe", data: { api_key: "ts_test_only" } });

    // A rerun reuses both: no second model, no replaced key.
    const again = fakeRequest((method, path) => {
      if (method === "GET" && path === MODELS_PATH) return { models: [modelRow], next_cursor: null };
      if (method === "GET" && path === SECRETS_PATH) return { secrets: [{ name: "typesafe" }] };
      return expect.unreachable(`a configured check must not write: ${method} ${path}`);
    });
    expect(await configureDecisions(again, FIXTURE_WORKSPACE)).toBe(DECISION_ID);
  });

  test("another platform workspace keeps its own key, and a missing key refuses before any write", async () => {
    process.env.TYPESAFE_API_KEY = "ts_test_only";
    const other = fakeRequest((method, path) => {
      if (method === "GET" && path === MODELS_PATH) return { models: [modelRow], next_cursor: null };
      return expect.unreachable(`must not touch another workspace: ${method} ${path}`);
    });
    expect(await configureDecisions(other, FIXTURE_WORKSPACE, "operator-workspace")).toBe(DECISION_ID);
    delete process.env.TYPESAFE_API_KEY;
    await expect(configureDecisions(fakeRequest(() => expect.unreachable("no key, no request")), FIXTURE_WORKSPACE))
      .rejects.toThrow(/TYPESAFE_API_KEY/);
  });
});
