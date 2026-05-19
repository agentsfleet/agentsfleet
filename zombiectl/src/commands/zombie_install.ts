// Zombie install + update — Effects mirroring the imperative leaves
// they replaced. Each reads the workspace-scoped MainLayer services
// (CliConfig + Output + HttpClient + Credentials + Workspaces) and
// emits CliError variants on failure.
//
// `loadSkillFromPath` is sync filesystem IO — wrapped in `Effect.try`
// so the SkillLoadError surfaces on the typed error channel as a
// ConfigError (operator can act on the message).

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsZombiesPath, wsZombiePath } from "../lib/api-paths.ts";
import {
  loadSkillFromPath,
  SkillLoadError,
  type LoadedSkill,
} from "../lib/load-skill-from-path.ts";
import { validateRequiredId } from "../program/validators.ts";
import { OPT_FROM } from "../constants/cli-flags.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";

interface InstallResponse {
  readonly zombie_id?: string;
  readonly name?: string;
  readonly webhook_urls?: Record<string, string>;
}

interface UpdateResponse {
  readonly config_revision?: number | string | null;
}

const USAGE_INSTALL = "zombiectl install --from <path>";
const USAGE_UPDATE =
  "zombiectl zombie update <zombie_id> --from <path>";

const loadBundle = (
  fromPath: string,
): Effect.Effect<LoadedSkill, ConfigError> =>
  Effect.try({
    try: () => loadSkillFromPath(fromPath),
    catch: (err) =>
      new ConfigError({
        detail:
          err instanceof SkillLoadError
            ? `${err.code}: ${err.message}`
            : err instanceof Error
              ? err.message
              : String(err),
        suggestion:
          "verify the path exists and contains a skill.md + trigger.md",
      }),
  });

const requireFromPath = (
  fromPath: string | null | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> => {
  if (typeof fromPath !== "string" || fromPath.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--from <path> is required",
        suggestion: `usage: ${usage}`,
      }),
    );
  }
  return Effect.succeed(fromPath);
};

export const installEffectFromFlags = (
  fromPath: string | null | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const path = yield* requireFromPath(fromPath, USAGE_INSTALL);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<InstallResponse>({
      path: wsZombiesPath(wsId),
      method: "POST",
      body: {
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      },
      token,
    });

    const displayName = res.name || bundle.fallback_name;

    if (config.jsonMode) {
      yield* output.printJson({
        status: "installed",
        zombie_id: res.zombie_id,
        webhook_urls: res.webhook_urls ?? {},
        name: displayName,
      });
      return;
    }

    yield* output.success(`${displayName} is live.`);
    if (res.zombie_id) yield* output.info(`  Zombie ID: ${res.zombie_id}`);
    const urls = res.webhook_urls ?? {};
    const sources = Object.keys(urls);
    if (sources.length > 0) {
      yield* output.info("  Webhook URLs (register on the upstream provider):");
      for (const source of sources) {
        yield* output.info(`    ${source}: ${urls[source]}`);
      }
    }
  });

export const updateEffectFromArgs = (
  zombieId: string | undefined,
  fromPath: string | null | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    if (!zombieId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "zombie_id is required",
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }
    const idCheck = validateRequiredId(zombieId, "zombie_id");
    if (!idCheck.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: idCheck.message,
          suggestion: `usage: ${USAGE_UPDATE}`,
        }),
      );
    }

    const path = yield* requireFromPath(fromPath, USAGE_UPDATE);
    const wsId = yield* requireWorkspaceId;
    const bundle = yield* loadBundle(path);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<UpdateResponse>({
      path: wsZombiePath(wsId, zombieId),
      method: "PATCH",
      body: {
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({
        status: "updated",
        zombie_id: zombieId,
        config_revision: res.config_revision,
      });
      return;
    }

    yield* output.success(`${zombieId} updated.`);
    if (res.config_revision != null) {
      yield* output.info(`  Config revision: ${res.config_revision}`);
    }
  });

export { OPT_FROM };
