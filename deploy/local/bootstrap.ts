import { chmod, lstat, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { createHash, randomBytes } from "node:crypto";
import { z } from "zod";
import { readCatalogue, CATALOGUE_URL } from "../../scripts/model-catalogue/catalogue";
import { desiredRows } from "../../scripts/model-catalogue/rows";
import { FIXTURE_KEY } from "../../ui/packages/app/tests/e2e/acceptance/fixtures/constants";

const TIMEOUT_MS = 20_000;
const HTTP_METHOD = {get: "GET", post: "POST"} as const;
const EXCLUSIVE_CREATE = "wx";
const PROVIDER = "fireworks";
const MODEL = "accounts/fireworks/models/minimax-m3p1-flash";
const PLATFORM_SCOPES = ["runner:enroll", "runner:read", "model:read", "model:admin", "platform-key:read", "platform-key:admin"];
const RUNNERS = "/v1/admin/runners";
const FLEET_RUNNERS = "/v1/admin/runners";
const MODELS = "/v1/admin/models";
const DECISION_PROVIDER = "typesafe";
// TypeSafe answers `jev-latest` with the version that served it, and only an
// accepted reported version counts: a new one is ignored until it is added here.
const DECISION_MODEL = "jev-latest";
const DECISION_ACCEPTED = ["jev-1.14.0"];
const DECISION_DOCS = "https://docs.typesafe.ai/api/system-one.md";
// One call per message on Nov 16, 2026: "Save" scored 0.11 and
// "Rename it to billing-digest" 0.81 for "does this ask to change the setup?".
const DECISION_EVIDENCE = '{"requested":"jev-latest","reported":"jev-1.14.0","save":0.11,"rename":0.81}';
const DECISION_VERIFIED_AT = "2026-11-16T10:30:00.000Z";
const PLATFORM_KEYS = "/v1/admin/platform-keys";
const GITHUB_FIELDS = ["app_id", "app_slug", "client_id", "client_secret", "private_key_pem", "webhook_secret"];
const SIGNING_SECRET = "approval-signing";
const SIGNING_BYTES = 32;
const SCHEDULER_BAG = "qstash";
/** The local QStash emulator's fixed login, as `@upstash/qstash-cli dev` ships it. */
const EMULATOR_LOGIN = {UserID: "defaultUser", Password: "defaultPassword"};
const emulatorKeysSchema = z.object({current: z.string().min(1), next: z.string().min(1)});
const portSchema = z.string().regex(/^[0-9]+$/);
const EMULATOR_SILENT = "the local QStash emulator did not answer its signing keys; start the shared infrastructure first";
const registrationSchema = z.object({ runner_id: z.uuidv7(), runner_token: z.string().startsWith("agt_r") });
const runnerSelfSchema = z.object({assigned_policy: z.object({decision_model_id: z.string().optional()})});
const runnerPageSchema = z.object({items: z.array(z.object({host_id: z.string()})), next_cursor: z.string().min(1).nullable()});
type Request = <T>(method: string, path: string, body?: unknown) => Promise<T>;
/** Runs the 1Password command line with these arguments and returns its output. */
type Op = (args: string[]) => Promise<string>;
class LocalBootstrapError extends Error {}

/** Refuse a remote target before creating identities or reading provider credentials. */
export function localTarget(value: string, expected: string): string {
  const url = new URL(value);
  if (url.origin !== expected || url.protocol !== "http:" || url.hostname !== "127.0.0.1"
    || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new LocalBootstrapError("acceptance bootstrap requires the worktree's loopback API");
  }
  return url.origin;
}

/** Never replace existing encrypted state with a new encryption key. */
export async function privateFile(path: string): Promise<void> {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.uid !== process.getuid?.() || (stat.mode & 0o077) !== 0)
    throw new LocalBootstrapError("acceptance state must be a private, owned regular file");
}

function api(base: string, token: string): Request {
  return async <T>(method: string, path: string, body?: unknown): Promise<T> => {
    const response = await fetch(`${base}${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    // Response bodies can contain credentials. Only the status leaves this boundary.
    if (!response.ok) throw new LocalBootstrapError(`local bootstrap ${method} ${path}: HTTP ${response.status}`);
    return response.json() as Promise<T>;
  };
}

function clerkAdmin() {
  return import("../../ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin");
}

async function identity(namespace: string) {
  const { bootstrapTenant } = await import("../../ui/packages/app/tests/e2e/acceptance/fixtures/bootstrap");
  const { attachJwt, finalizeFixtureMetadata, provisionUser } = await clerkAdmin();
  const { clerkRequest } = await import("../../ui/packages/app/tests/e2e/acceptance/fixtures/clerk-request");
  const fixture = await provisionUser({ key: FIXTURE_KEY.policyEditor,
    email: `local-platform-${namespace}@e2e.agentsfleet.net`, password: crypto.randomUUID() });
  await bootstrapTenant(fixture);
  await finalizeFixtureMetadata(fixture);
  const path = `/users/${fixture.clerkUserId}/metadata`;
  const user = await clerkRequest<{ public_metadata: { scopes: string | string[] } }>(HTTP_METHOD.get, `/users/${fixture.clerkUserId}`);
  const current = user.public_metadata.scopes;
  const scopes = typeof current === "string" ? current.split(/\s+/).filter(Boolean) : current;
  await clerkRequest("PATCH", path, { public_metadata: { scopes: [...new Set([...scopes, ...PLATFORM_SCOPES])].join(" ") } });
  return attachJwt(fixture);
}

async function workspace(request: Request): Promise<string> {
  const result = await request<{ items: { id: string; role: string }[] }>(HTTP_METHOD.get, "/v1/tenants/me/workspaces");
  const owned = result.items.filter((row) => row.role === "owner");
  if (owned.length !== 1) throw new LocalBootstrapError("local platform fixture must own exactly one workspace");
  return z.uuidv7().parse(owned[0]?.id);
}

async function saveSecret(request: Request, workspaceId: string, name: string, data: Record<string, string>): Promise<void> {
  const path = `/v1/workspaces/${workspaceId}/secrets`;
  const { secrets } = await request<{ secrets: { name: string }[] }>(HTTP_METHOD.get, path);
  if (!secrets.some((row) => row.name === name)) await request(HTTP_METHOD.post, path, { name, data });
}

async function catalogueModel(request: Request, provider: string, model: string): Promise<string | undefined> {
  const schema = z.object({ models: z.array(z.object({ id: z.string(), provider: z.string(), model_id: z.string() })),
    next_cursor: z.string().min(1).nullable() });
  const seen = new Set<string>();
  let cursor: string | null = null;
  do {
    const suffix: string = cursor ? `?starting_after=${encodeURIComponent(cursor)}` : "";
    const page = schema.parse(await request<unknown>(HTTP_METHOD.get, `${MODELS}${suffix}`));
    const found = page.models.find((entry) => entry.provider === provider && entry.model_id === model);
    if (found) return found.id;
    cursor = page.next_cursor;
    if (cursor && seen.has(cursor)) throw new LocalBootstrapError("model catalogue repeated a cursor");
    if (cursor) seen.add(cursor);
  } while (cursor);
  return undefined;
}

async function configureModel(request: Request, workspaceId: string): Promise<void> {
  const { keys } = await request<{ keys: { provider: string; active: boolean }[] }>(HTTP_METHOD.get, PLATFORM_KEYS);
  if (keys.some((key) => key.active)) return;
  const key = process.env.FIREWORKS_API_KEY;
  if (!key) throw new LocalBootstrapError("machine configuration must supply FIREWORKS_API_KEY for initial local setup");
  const row = desiredRows(await readCatalogue(CATALOGUE_URL)).find((entry) => entry.provider === PROVIDER && entry.model_name === MODEL);
  if (!row) throw new LocalBootstrapError("local acceptance model is missing from the checked catalogue");
  if (!await catalogueModel(request, PROVIDER, MODEL)) {
    const { provider, model_name, prices, inference, ...metadata } = row;
    await request(HTTP_METHOD.post, MODELS, { provider, model_id: model_name, metadata, prices, inference });
  }
  await saveSecret(request, workspaceId, PROVIDER, { provider: PROVIDER, api_key: key, model: MODEL });
  await request("PUT", PLATFORM_KEYS, { provider: PROVIDER, source_workspace_id: workspaceId, model: MODEL });
}

/** Runs only on a fresh acceptance database, so later runs never touch 1Password. */
/**
 * The setup edit check asks this model whether a chat message requests a setup
 * change, so a plain "Save" is never mistaken for an edit. Its key lives in the
 * platform workspace, the only place the daemon reads decision keys from.
 */
export async function configureDecisions(request: Request, workspaceId: string, configuredWorkspace?: string): Promise<string> {
  const key = process.env.TYPESAFE_API_KEY;
  if (!key) throw new LocalBootstrapError("machine configuration must supply TYPESAFE_API_KEY for the setup edit check");
  const id = await catalogueModel(request, DECISION_PROVIDER, DECISION_MODEL)
    // The created model's fields are flattened into the reply (AdminModelCreated).
    ?? z.object({ id: z.string() }).parse(await request<unknown>(HTTP_METHOD.post, MODELS, decisionModel())).id;
  if (!configuredWorkspace || configuredWorkspace === workspaceId)
    await saveSecret(request, workspaceId, DECISION_PROVIDER, { api_key: key });
  return id;
}

/** A decision-only catalogue entry: no chat inference, one reviewed TypeSafe profile. */
export function decisionModel() {
  const at = DECISION_VERIFIED_AT;
  return {
    provider: DECISION_PROVIDER, model_id: DECISION_MODEL, prices: [],
    metadata: { display_name: "Setup edit check", advanced: true, context_cap_tokens: 8_192,
      context_source_url: DECISION_DOCS, context_verified_at: at },
    decision: {
      enabled: true,
      profile: {
        revision: `acceptance-${DECISION_ACCEPTED.join("+")}`, api: DECISION_PROVIDER, endpoint: "https://api.typesafe.ai/v1/systemone",
        accepted_models: DECISION_ACCEPTED, capabilities: { noul: true, choice: true, score: true }, input_max_bytes: 24_576,
        intent: { questions_revision: "dt-routing-v1", intent: { probability_bps: 8500, confidence_bps: 7000 },
          association: { probability_bps: 9500, confidence_bps: 8500 }, single_request_max_bps: 3000, multiple_request_min_bps: 7000 },
        goal: { questions_revision: "dt-goal-v1", statement_min_bps: 9500,
          continue_action: { probability_bps: 8000, confidence_bps: 7000 }, wait: { probability_bps: 8500, confidence_bps: 7500 },
          done: { probability_bps: 9500, confidence_bps: 8500 } },
        evaluation_digest: createHash("sha256").update(DECISION_EVIDENCE).digest("hex"),
        evaluation_source_url: DECISION_DOCS, evaluated_at: at,
      },
      // TypeSafe publishes no input rate and bills no output tokens; this rate
      // only meters local acceptance.
      prices: [{ effective_from: at, effective_until: null, prompt_tokens_above: 0,
        input_nanos_per_mtok: "1000000000", cached_input_nanos_per_mtok: "1000000000", cache_write_nanos_per_mtok: null,
        cache_write_5m_nanos_per_mtok: null, cache_write_1h_nanos_per_mtok: null, output_nanos_per_mtok: "0",
        source_url: DECISION_DOCS, verified_at: at,
        conditions: "Local acceptance only: TypeSafe publishes no input rate; output tokens are free" }],
    },
  };
}

export async function configureConnector(request: Request, workspaceId: string, op: Op = runOp): Promise<void> {
  await saveSecret(request, workspaceId, "github-app", await connectorFields(op));
}

/** A caller that resolved every field (a burst machine) passes them in; otherwise 1Password reads them all. */
async function connectorFields(op: Op): Promise<Record<string, string>> {
  const given: Record<string, string> = Object.fromEntries(
    GITHUB_FIELDS.map(field => [field, process.env[`LOCAL_GITHUB_${field.toUpperCase()}`] ?? ""]));
  if (Object.values(given).every(Boolean)) return given;
  const vault = process.env.ACCEPTANCE_LOCAL_CONNECTOR_VAULT || await connectorVault(op);
  const data: Record<string, string> = {};
  for (const field of GITHUB_FIELDS) data[field] = await op(["read", "--no-newline", `op://${vault}/github-app/${field}`]);
  if (!Object.values(data).every(Boolean)) throw new LocalBootstrapError("development GitHub App fields are required for initial connector setup");
  return data;
}

/** The one development vault holding the GitHub App item. */
async function connectorVault(op: Op): Promise<string> {
  const rows = z.array(z.object({title: z.string(), vault: z.object({id: z.string(), name: z.string().default("")})}))
    .parse(JSON.parse(await op(["item", "list", "--format", "json"])));
  const matches = rows.filter(row => row.title === "github-app" && /(?:^|_)DEV$/.test(row.vault.name));
  const [match] = matches;
  if (matches.length !== 1 || !match) throw new LocalBootstrapError("set ACCEPTANCE_LOCAL_CONNECTOR_VAULT to the development vault holding github-app");
  return match.vault.id;
}

async function runOp(args: string[]): Promise<string> {
  const child = Bun.spawn(["op", ...args], {stdin: "ignore", stdout: "pipe", stderr: "inherit", signal: AbortSignal.timeout(TIMEOUT_MS)});
  const output = await new Response(child.stdout).text();
  if (await child.exited !== 0) throw new LocalBootstrapError(`1Password could not run: op ${args[0]} ${args[1] ?? ""}`.trim());
  return output;
}

/** Only the dedicated local fixture may gain a signing bag; retries keep its key. */
export async function configureSigning(request: Request, workspaceId: string, configuredWorkspace?: string): Promise<void> {
  if (configuredWorkspace && configuredWorkspace !== workspaceId) return;
  await saveSecret(request, workspaceId, SIGNING_SECRET, {webhook_secret: randomBytes(SIGNING_BYTES).toString("hex")});
}

/** The fixture's scheduler credentials, read from the local QStash emulator on `port`; retries keep the first bag. */
export async function configureScheduler(request: Request, workspaceId: string, port: string, configuredWorkspace?: string): Promise<void> {
  if (configuredWorkspace && configuredWorkspace !== workspaceId) return;
  const url = `http://127.0.0.1:${portSchema.parse(port)}/v2`;
  const token = Buffer.from(JSON.stringify(EMULATOR_LOGIN)).toString("base64");
  let answer: Response;
  try {
    answer = await fetch(`${url}/keys`, {headers: {Authorization: `Bearer ${token}`}, signal: AbortSignal.timeout(TIMEOUT_MS)});
  } catch (error) {
    throw new LocalBootstrapError(EMULATOR_SILENT, {cause: error});
  }
  if (!answer.ok) throw new LocalBootstrapError(EMULATOR_SILENT);
  const keys = emulatorKeysSchema.parse(await answer.json());
  await saveSecret(request, workspaceId, SCHEDULER_BAG,
    {token, url, current_signing_key: keys.current, next_signing_key: keys.next});
}

async function requireFreshHost(request: Request, host: string): Promise<void> {
  const seen = new Set<string>();
  let cursor: string | null = null;
  do {
    const suffix: string = cursor ? `?starting_after=${encodeURIComponent(cursor)}` : "";
    const page = runnerPageSchema.parse(await request<unknown>(HTTP_METHOD.get, `${FLEET_RUNNERS}${suffix}`));
    if (page.items.some(row => row.host_id === host))
      throw new LocalBootstrapError("local runner already enrolled without its credential record; recover it through operator token rotation before retrying");
    cursor = page.next_cursor;
    if (cursor && seen.has(cursor)) throw new LocalBootstrapError("local runner list repeated a cursor; refusing enrollment");
    if (cursor) seen.add(cursor);
  } while (cursor);
}

export async function enroll(request: Request, base: string, namespace: string, record: string, decisionModelId: string): Promise<void> {
  if (await Bun.file(record).exists()) {
    await privateFile(record);
    const { runner_token } = registrationSchema.parse(await Bun.file(record).json());
    const self = runnerSelfSchema.parse(await api(base, runner_token)(HTTP_METHOD.get, "/v1/runners/me"));
    // Enrollment is the only place bootstrap sets the policy, so a runner
    // enrolled earlier keeps running without the setup edit check.
    if (self.assigned_policy.decision_model_id !== decisionModelId)
      throw new LocalBootstrapError("the reused acceptance runner names no setup edit check model; assign it the decision model in the runner configuration or recreate this acceptance database");
    return;
  }
  const pending = `${record}.enrolling`;
  const temporary = `${record}.new`;
  if (await Bun.file(pending).exists() || await Bun.file(temporary).exists())
    throw new LocalBootstrapError("local runner enrollment is unfinished; inspect its outcome and recover its credential before retrying");
  const host = `local-acceptance-${namespace}`;
  await requireFreshHost(request, host);
  await writeFile(pending, host, {flag: EXCLUSIVE_CREATE, mode: 0o600});
  const registration = await request(HTTP_METHOD.post, RUNNERS, { host_id: host, labels: [],
    assigned_policy: { durable_tasks_enabled: true, decision_model_id: decisionModelId, sandbox_tier: "container_nested",
      network_policy: "allow_all", registry_allowlist: [], worker_count: 4, extra_binds: [] } });
  registrationSchema.parse(registration);
  await writeFile(temporary, `${JSON.stringify(registration)}\n`, { flag: EXCLUSIVE_CREATE, mode: 0o600 });
  await rename(temporary, record);
  await unlink(pending);
}

export async function persistWorkspace(envFile: string, workspaceId: string): Promise<void> {
  await privateFile(envFile);
  const current = await readFile(envFile, "utf8");
  const existing = current.match(/^(?:export )?PLATFORM_ADMIN_WORKSPACE_ID=(.*)$/m);
  if (existing) {
    if (existing[1]?.trim() === workspaceId) return;
    throw new LocalBootstrapError("refusing to replace the configured platform credential workspace");
  }
  const next = current.trimEnd() + `\nPLATFORM_ADMIN_WORKSPACE_ID=${workspaceId}\n`;
  await writeFile(`${envFile}.new`, next, { flag: EXCLUSIVE_CREATE, mode: 0o600 });
  await rename(`${envFile}.new`, envFile);
  await chmod(envFile, 0o600);
}

export async function bootstrap(): Promise<void> {
  const base = localTarget(process.env.NEXT_PUBLIC_API_URL ?? "", process.env.LOCAL_ACCEPTANCE_API ?? "");
  const namespace = z.string().regex(/^wt_[a-f0-9]{8}$/).parse(process.env.LOCAL_ACCEPTANCE_NAMESPACE);
  const record = z.string().min(1).parse(process.env.ACCEPTANCE_LOCAL_RUNNER_RECORD);
  const envFile = z.string().min(1).parse(process.env.LOCAL_DEV_ENV_FILE);
  const session = await identity(namespace);
  const { revokeSession } = await clerkAdmin();
  try {
    const request = api(base, session.sessionJwt);
    const workspaceId = await workspace(request);
    await configureModel(request, workspaceId);
    const decision = await configureDecisions(request, workspaceId, process.env.PLATFORM_ADMIN_WORKSPACE_ID);
    await configureSigning(request, workspaceId, process.env.PLATFORM_ADMIN_WORKSPACE_ID);
    await configureScheduler(request, workspaceId, process.env.AGENTSFLEET_QSTASH_HOST_PORT ?? "", process.env.PLATFORM_ADMIN_WORKSPACE_ID);
    if (process.env.PLATFORM_ADMIN_WORKSPACE_ID) {
      const entries = await request<{ configured: boolean }[]>(HTTP_METHOD.get, `/v1/workspaces/${workspaceId}/connectors`);
      if (!entries.some(entry => entry.configured))
        throw new LocalBootstrapError("configured platform credential workspace has no ready connector; repair its provider bags before acceptance");
    } else {
      await configureConnector(request, workspaceId);
      await persistWorkspace(envFile, workspaceId);
    }
    await enroll(request, base, namespace, record, decision);
    console.log("Local platform model, decision model, connector, scheduler and runner configuration ready.");
  } finally {
    await revokeSession(session.sessionId);
  }
}

if (import.meta.main) {
  try { await bootstrap(); }
  catch (error) {
    // Provider bodies and fixture tokens must never reach shell or CI logs.
    if (error instanceof LocalBootstrapError) console.error(error.message);
    console.error(`Local acceptance bootstrap failed (${error instanceof Error ? error.name : "unknown error"}); credentials withheld.`);
    process.exitCode = 1;
  }
}
