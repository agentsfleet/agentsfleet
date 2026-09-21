// Shared doubles for the post-login workspace hydration tests.
//
// Extracted when login-helpers-hydration.unit.test.ts passed the repository's
// 350-line cap.

import { Effect, Layer, Redacted } from "effect";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { Workspaces, type WorkspacesValue } from "../src/services/workspaces.ts";
import { NetworkError, ServerError, UnexpectedError } from "../src/errors/index.ts";

export interface Rec {
  readonly stderr: string[];
  saved: number;
  savedValue: WorkspacesValue | null;
}

export const makeRec = (): Rec => ({ stderr: [], saved: 0, savedValue: null });

export const outputLayer = (rec: Rec): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    ...outputDouble(),
    warn: (msg) => Effect.sync(() => rec.stderr.push(msg)),
  });

export const httpLayer = (
  responder: (
    input: HttpRequestInput,
  ) => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input: HttpRequestInput) =>
      input.path.startsWith("/v1/tenants/me/workspaces?")
        ? (responder(input) as Effect.Effect<
            never,
            NetworkError | ServerError
          >)
        : Effect.die(`unexpected ${input.path}`),
  });

export const workspacesLayer = (
  rec: Rec,
  saveResult: Effect.Effect<void, UnexpectedError> = Effect.void,
  loadResult: Workspaces["load"] = Effect.succeed({
    current_workspace_id: null,
    items: [],
  }),
): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: loadResult,
    save: (next) =>
      saveResult.pipe(
        Effect.tap(() =>
          Effect.sync(() => {
            rec.saved += 1;
            rec.savedValue = next;
          }),
        ),
      ),
  });

export const tok = Redacted.make("opaque-direct-token");
