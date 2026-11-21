import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { fetchGates } from "../src/commands/approvals/approvals.ts";
import { ServerError } from "../src/errors/index.ts";
import { HttpClient } from "../src/services/http-client.ts";

// The page bound the walk once carried; a longer inbox is now read whole.
const RETIRED_PAGE_CAP = 50;

const pagedHttp = (next: (page: number) => string | null, paths: string[] = []) => Layer.succeed(HttpClient, {
  request: <T>(input: {path: string}) => Effect.sync(() => {
    paths.push(input.path);
    return {items: [{gate_id: `gate-${paths.length}`}], next_cursor: next(paths.length)} as T;
  }),
});

test("walks every approval page past the retired cap until the server reports the end", async () => {
  const pages = RETIRED_PAGE_CAP + 5;
  const paths: string[] = [];
  const gates = await Effect.runPromise(fetchGates("workspace", Redacted.make("test-token"), {}).pipe(
    Effect.provide(pagedHttp((page) => page < pages ? `page-${page}` : null, paths))));
  expect(gates).toHaveLength(pages);
  expect(paths).toHaveLength(pages);
  expect(paths[pages - 1]).toContain(`cursor=page-${pages - 1}`);
});

test("refuses a page cursor the walk already followed instead of walking forever", async () => {
  const paths: string[] = [];
  const result = await Effect.runPromiseExit(fetchGates("workspace", Redacted.make("test-token"), {}).pipe(
    Effect.provide(pagedHttp(() => "same", paths))));
  expect(paths).toHaveLength(2);
  expect(Exit.isFailure(result)).toBe(true);
  if (Exit.isFailure(result)) {
    const error = Option.getOrNull(Cause.findErrorOption(result.cause));
    expect(error).toBeInstanceOf(ServerError);
    expect(error).toMatchObject({detail: "the approval list repeated page cursor same", code: "APPROVAL_LIST_CURSOR_REPEATED", status: 502});
  }
});
