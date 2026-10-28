import { chmod, lstat, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { randomBytes } from "node:crypto";
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
const RUNNERS = "/v1/runners";
const FLEET_RUNNERS = "/v1/fleets/runners";
const MODELS = "/v1/admin/models";
const PLATFORM_KEYS = "/v1/admin/platform-keys";
const GITHUB_FIELDS = ["app_id", "app_slug", "client_id", "client_secret", "private_key_pem", "webhook_secret"];
const SIGNING_SECRET = "approval-signing";
const SIGNING_BYTES = 32;
const registrationSchema = z.object({ runner_id: z.uuidv7(), runner_token: z.string().startsWith("agt_r") });
const runnerPageSchema = z.object({items: z.array(z.object({host_id: z.string()})), next_cursor: z.string().min(1).nullable()});
type Request = <T>(method: string, path: string, body?: unknown) => Promise<T>;
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

async function configureModel(request: Request, workspaceId: string): Promise<void> {
  const { keys } = await request<{ keys: { provider: string; active: boolean }[] }>(HTTP_METHOD.get, PLATFORM_KEYS);
  if (keys.some((key) => key.active)) return;
  const key = process.env.FIREWORKS_API_KEY;
  if (!key) throw new LocalBootstrapError("machine configuration must supply FIREWORKS_API_KEY for initial local setup");
  const row = desiredRows(await readCatalogue(CATALOGUE_URL)).find((entry) => entry.provider === PROVIDER && entry.model_name === MODEL);
  if (!row) throw new LocalBootstrapError("local acceptance model is missing from the checked catalogue");
  const { models } = await request<{ models: { provider: string; model_id: string }[] }>(HTTP_METHOD.get, MODELS);
  if (!models.some((model) => model.provider === PROVIDER && model.model_id === MODEL)) {
    const { provider, model_name, prices, inference, ...metadata } = row;
    await request(HTTP_METHOD.post, MODELS, { provider, model_id: model_name, metadata, prices, inference });
  }
  await saveSecret(request, workspaceId, PROVIDER, { provider: PROVIDER, api_key: key, model: MODEL });
  await request("PUT", PLATFORM_KEYS, { provider: PROVIDER, source_workspace_id: workspaceId, model: MODEL });
}

async function configureConnector(request: Request, workspaceId: string): Promise<void> {
  const data: Record<string, string> = {};
  for (const field of GITHUB_FIELDS) {
    const value = process.env[`LOCAL_GITHUB_${field.toUpperCase()}`];
    if (!value) throw new LocalBootstrapError("development GitHub App fields are required for initial connector setup");
    data[field] = value;
  }
  await saveSecret(request, workspaceId, "github-app", data);
}

/** Only the dedicated local fixture may gain a signing bag; retries keep its key. */
export async function configureSigning(request: Request, workspaceId: string, configuredWorkspace?: string): Promise<void> {
  if (configuredWorkspace && configuredWorkspace !== workspaceId) return;
  await saveSecret(request, workspaceId, SIGNING_SECRET, {webhook_secret: randomBytes(SIGNING_BYTES).toString("hex")});
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

export async function enroll(request: Request, base: string, namespace: string, record: string): Promise<void> {
  if (await Bun.file(record).exists()) {
    await privateFile(record);
    const { runner_token } = registrationSchema.parse(await Bun.file(record).json());
    await api(base, runner_token)(HTTP_METHOD.get, "/v1/runners/me");
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
    assigned_policy: { sandbox_tier: "container_nested", network_policy: "allow_all", registry_allowlist: [], worker_count: 4, extra_binds: [] } });
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
    await configureSigning(request, workspaceId, process.env.PLATFORM_ADMIN_WORKSPACE_ID);
    if (process.env.PLATFORM_ADMIN_WORKSPACE_ID) {
      const entries = await request<{ configured: boolean }[]>(HTTP_METHOD.get, `/v1/workspaces/${workspaceId}/connectors`);
      if (!entries.some(entry => entry.configured))
        throw new LocalBootstrapError("configured platform credential workspace has no ready connector; repair its provider bags before acceptance");
    } else {
      await configureConnector(request, workspaceId);
      await persistWorkspace(envFile, workspaceId);
    }
    await enroll(request, base, namespace, record);
    console.log("Local platform model, connector and runner configuration ready.");
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
