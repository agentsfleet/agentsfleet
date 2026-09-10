/**
 * Shared template-onboarding helpers for the acceptance seed + negative paths.
 *
 * The two-tier model has no CLI onboard verb, so fixtures onboard templates over
 * HTTP directly (mirrors `secret-ops.ts`), reading auth — API URL, bearer
 * token, workspace — from the same state dir the CLI run authenticates against
 * (`AGENTSFLEET_STATE_DIR`). Both the seed (`install --library`) and the
 * duplicate-name negative onboard the canonical `platform-ops` sample as a tenant
 * template (`source_kind: "upload"`), differing only in whether the name is
 * randomized per call or held stable across the duplicate installs.
 */

import path from "node:path";
import url from "node:url";
import fs from "node:fs/promises";

import {
  ACCEPTANCE_PROVIDER_MODEL,
  PLATFORM_OPS_FIXTURE_NAME,
  PLATFORM_OPS_SAMPLE_DIR,
  STEER_PROBE_FIXTURE_NAME,
  STEER_PROBE_SAMPLE_DIR,
} from "./constants.ts";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const WORKTREE_ROOT = path.resolve(HERE, "..", "..", "..", "..");
const ENV_API_URL = "AGENTSFLEET_API_URL";
const ENV_STATE_DIR = "AGENTSFLEET_STATE_DIR";
const SOURCE_KIND_UPLOAD = "upload";
const DEFAULT_ONBOARD_TIMEOUT_MS = 60_000;

export interface AuthContext {
  readonly apiUrl: string;
  readonly token: string;
  readonly workspaceId: string;
}

export interface SampleContent {
  readonly skillMarkdown: string;
  readonly triggerMarkdown: string;
}

// Read the API URL + bearer token + workspace from the run's state dir, so the
// onboard call carries the run's own identity (the same the CLI install uses).
export async function readAuthContext(env: Readonly<Record<string, string>>): Promise<AuthContext> {
  const apiUrl = env[ENV_API_URL];
  const stateDir = env[ENV_STATE_DIR];
  if (!apiUrl) throw new Error(`onboard requires ${ENV_API_URL} in the composed env`);
  if (!stateDir) throw new Error(`onboard requires ${ENV_STATE_DIR} in the composed env`);
  const credentials = JSON.parse(
    await fs.readFile(path.join(stateDir, "credentials.json"), "utf8"),
  ) as { token?: string | null };
  const workspaces = JSON.parse(
    await fs.readFile(path.join(stateDir, "workspaces.json"), "utf8"),
  ) as { current_workspace_id?: string | null };
  if (!credentials.token) throw new Error("onboard: no token in state-dir credentials.json");
  if (!workspaces.current_workspace_id) {
    throw new Error("onboard: no current_workspace_id in state-dir workspaces.json");
  }
  return { apiUrl, token: credentials.token, workspaceId: workspaces.current_workspace_id };
}

// Build the canonical sample SKILL.md/TRIGGER.md with a caller-chosen frontmatter
// `name:` and the two frontmatter template tokens resolved, so the upload parses
// server-side. Body-only placeholders never reach the config parser.
export async function buildPlatformOpsContent(name: string): Promise<SampleContent> {
  return buildFixtureContent(PLATFORM_OPS_SAMPLE_DIR, PLATFORM_OPS_FIXTURE_NAME, name);
}

export async function buildSteerProbeContent(
  name: string,
  model: string = ACCEPTANCE_PROVIDER_MODEL,
): Promise<SampleContent> {
  return buildFixtureContent(STEER_PROBE_SAMPLE_DIR, STEER_PROBE_FIXTURE_NAME, name, model);
}

// What the steer probe's own frontmatter says where it declares nothing.
const EMPTY_CREDENTIALS_BLOCK = "  credentials: []";

/**
 * The steer probe, plus one declared credential.
 *
 * The same bundle as `buildSteerProbeContent` — same model pin, same network
 * allowance, same one-line reply instruction — with its empty `credentials:`
 * list replaced by a name. That single edit is what makes the install raise an
 * integration-grant card: the daemon classifies each declared name against the
 * workspace's stored handle, and a handle carrying `integration: <connector>`
 * is one the runner must MINT rather than receive.
 *
 * Composed rather than given its own sample directory so the two bundles cannot
 * drift on the model or the host allowance, which is what every live dial here
 * actually depends on.
 */
export async function buildConnectorProbeContent(
  name: string,
  credentialName: string,
): Promise<SampleContent> {
  const probe = await buildSteerProbeContent(name);
  if (!probe.triggerMarkdown.includes(EMPTY_CREDENTIALS_BLOCK)) {
    throw new Error(
      `the steer-probe TRIGGER.md no longer carries "${EMPTY_CREDENTIALS_BLOCK}"; ` +
        "the connector variant cannot declare its credential",
    );
  }
  return {
    skillMarkdown: probe.skillMarkdown,
    triggerMarkdown: probe.triggerMarkdown.replace(
      EMPTY_CREDENTIALS_BLOCK,
      `  credentials:\n    - ${credentialName}`,
    ),
  };
}

async function buildFixtureContent(
  sampleDir: string,
  fixtureName: string,
  name: string,
  model: string = ACCEPTANCE_PROVIDER_MODEL,
): Promise<SampleContent> {
  const sourceDir = path.join(WORKTREE_ROOT, sampleDir);
  const skill = await fs.readFile(path.join(sourceDir, "SKILL.md"), "utf8");
  const trigger = await fs.readFile(path.join(sourceDir, "TRIGGER.md"), "utf8");
  return {
    skillMarkdown: skill
      .replace(`name: ${fixtureName}`, `name: ${name}`)
      .replaceAll("{{slack_channel}}", "#agentsfleet-acceptance"),
    triggerMarkdown: trigger
      .replace(`name: ${fixtureName}`, `name: ${name}`)
      .replaceAll("{{model}}", model)
      .replaceAll("{{context_cap_tokens}}", "256000"),
  };
}

// Onboard the content as a tenant template (upload kind) and return its id.
export async function onboardUploadTemplate(
  ctx: AuthContext,
  content: SampleContent,
  timeoutMs = DEFAULT_ONBOARD_TIMEOUT_MS,
): Promise<string> {
  const res = await fetch(
    `${ctx.apiUrl}/v1/workspaces/${encodeURIComponent(ctx.workspaceId)}/fleet-libraries`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${ctx.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        source_kind: SOURCE_KIND_UPLOAD,
        skill_markdown: content.skillMarkdown,
        trigger_markdown: content.triggerMarkdown,
      }),
      signal: AbortSignal.timeout(timeoutMs),
    },
  );
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(`template onboard ${res.status}: ${detail.slice(0, 200)}`);
  }
  const body = (await res.json()) as { id?: string };
  if (!body.id) throw new Error(`template onboard returned no id: ${JSON.stringify(body)}`);
  return body.id;
}
