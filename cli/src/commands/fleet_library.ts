// `agentsfleet library` — browse the first-party Fleet library catalog
// (the CLI peer of the dashboard's install gallery). Global, not workspace-
// scoped: GET /v1/fleets/bundles returns metadata only (id/name/description +
// declared requirement names); the SKILL.md/TRIGGER.md content is fetched
// server-side at snapshot time. Pick one with `agentsfleet install --library <id>`.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { FLEET_BUNDLES_PATH } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import type { CliError } from "../errors/index.ts";

interface FleetLibrary {
  readonly id?: string;
  readonly name?: string;
  readonly description?: string;
  readonly required_credentials?: ReadonlyArray<string>;
  readonly required_tools?: ReadonlyArray<string>;
  readonly network_hosts?: ReadonlyArray<string>;
}

interface FleetLibraryListResponse {
  readonly items?: ReadonlyArray<FleetLibrary>;
}

const FIELD_ID = "id" as const;
const FIELD_NAME = "name" as const;
const FIELD_CREDENTIALS = "credentials" as const;
const EMPTY_REQUIREMENT = "—" as const;

const joinNames = (names: ReadonlyArray<string> | undefined): string =>
  names && names.length > 0 ? names.join(", ") : EMPTY_REQUIREMENT;

export const libraryEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;

  const token = yield* resolveAuthToken;
  const res = yield* http.request<FleetLibraryListResponse>({
    path: FLEET_BUNDLES_PATH,
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }

  const items = res.items ?? [];
  if (items.length === 0) {
    yield* output.info("No fleet library yet.");
    return;
  }

  yield* output.printTable(
    [
      { key: FIELD_ID, label: "LIBRARY" },
      { key: FIELD_NAME, label: "NAME" },
      { key: FIELD_CREDENTIALS, label: "SECRETS" },
    ],
    items.map((t) => ({
      id: String(t.id ?? ""),
      name: String(t.name ?? ""),
      credentials: joinNames(t.required_credentials),
    })),
  );
  yield* output.info(
    ui.dim("Install one with: agentsfleet install --library <library>"),
  );
});
