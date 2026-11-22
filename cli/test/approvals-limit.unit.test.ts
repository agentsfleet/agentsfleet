import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { fetchGates } from "../src/commands/approvals/approvals.ts";
import { ServerError } from "../src/errors/index.ts";
import { HttpClient } from "../src/services/http-client.ts";

const pagedHttp = (next: (page: number) => string | null, paths: string[] = []) => Layer.succeed(HttpClient, {
  request: <T>(input: {path: string}) => Effect.sync(() => {
    paths.push(input.path);
    return {items: [{gate_id: `gate-${paths.length}`}], next_cursor: next(paths.length)} as T;
  }),
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

test("an approvals page names no limit, so the server's ceiling decides", async () => {
  const paths: string[] = [];
  const gates = await Effect.runPromise(fetchGates("workspace", Redacted.make("test-token"), {fleetId: "fleet", status: "pending"}).pipe(
    Effect.provide(pagedHttp((page) => (page === 1 ? "next" : null), paths))));
  const queries = paths.map((path) => new URLSearchParams(path.slice(path.indexOf("?") + 1)));
  expect(gates).toHaveLength(2);
  expect(queries.map((query) => query.has("limit"))).toEqual([false, false]);
  expect(queries.map((query) => query.get("fleet_id"))).toEqual(["fleet", "fleet"]);
  expect(queries.map((query) => query.get("status"))).toEqual(["pending", "pending"]);
  expect(queries.map((query) => query.get("cursor"))).toEqual([null, "next"]);
});
