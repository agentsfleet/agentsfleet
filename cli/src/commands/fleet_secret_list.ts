// `agentsfleet secret list` — the workspace vault's index.
//
// Split from fleet_secret.ts to keep that file inside the 350-line cap once the
// list grew a real table. It renders names, kinds and creation times only; the
// stored bytes never reach this module, which is what makes the "never echoes
// secret bytes" claim checkable by reading one short file.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsSecretsPath } from "../lib/api-paths.ts";
import type { CliError } from "../errors/index.ts";
import { EMPTY_CELL } from "../output/index.ts";

/** One vault row. `kind` says whether the value is a provider credential or a
 *  custom object; the daemon has always sent it and the list never read it. */
interface SecretRow {
  readonly name?: string;
  readonly created_at?: string | number | null;
  readonly kind?: string | null;
}

interface SecretsListResponse {
  readonly secrets?: ReadonlyArray<SecretRow>;
}

const FIELD_NAME = "name" as const;
const FIELD_KIND = "kind" as const;
const FIELD_CREATED = "created" as const;
const SECRETS_LISTED = "Workspace secrets" as const;
const EMPTY_VAULT =
  "No secrets stored. Create one with: agentsfleet secret create <name> --data=@- (pipe JSON on stdin)" as const;

export const secretListEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;

  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const res = yield* http.request<SecretsListResponse>({
    path: wsSecretsPath(wsId),
    token,
  });

  if (output.format !== OUTPUT_FORMAT.text) {
    // Names, kinds and stamps only — the same rows the table shows. The stored
    // bytes never reach this module in either register.
    yield* output.success(SECRETS_LISTED, { ...res });
    return;
  }
  const secrets = res.secrets ?? [];
  if (secrets.length === 0) {
    yield* output.info(EMPTY_VAULT);
    return;
  }
  // A header and aligned columns, matching `api-key list` and `grant list`.
  // This printed two space-separated fields and a raw epoch integer, so the one
  // list a person reads while handling credentials was the one that did not
  // look like the others.
  yield* output.printTable(
    [
      { key: FIELD_NAME, label: "NAME" },
      { key: FIELD_KIND, label: "KIND" },
      { key: FIELD_CREATED, label: "CREATED" },
    ],
    secrets.map((row) => ({
      name: row.name ?? "",
      kind: row.kind ?? EMPTY_CELL,
      created: row.created_at
        ? new Date(row.created_at).toISOString()
        : EMPTY_CELL,
    })),
  );
});
