import { expect, test } from "bun:test";
import { Effect } from "effect";
import { RetainedRequest } from "../../src/commands/chat/retained-request.ts";
import { NetworkError, ServerError } from "../../src/errors/index.ts";

const IDENTITY = {message: "same request"};
const network = new NetworkError({detail: "answer lost", suggestion: "retry", url: "http://local.test"});

test("unknown outcomes keep the exact operation key and reject changed intent", async () => {
  const request = new RetainedRequest<string>();
  const keys: string[] = [];
  let calls = 0;
  const execute = (key: string) => Effect.suspend(() => {
    keys.push(key);
    return calls++ === 0 ? Effect.fail(network) : Effect.succeed("accepted");
  });
  expect(await Effect.runPromise(Effect.flip(request.run(IDENTITY, execute)))).toBe(network);
  expect(request.pending).toBe(true);
  const changed = await Effect.runPromise(Effect.flip(request.run({message: "changed"}, execute)));
  expect(changed.detail).toContain("unknown outcome");
  expect(calls).toBe(1);
  expect(await Effect.runPromise(request.retry())).toBe("accepted");
  expect(keys).toHaveLength(2);
  expect(keys[0]).toBe(keys[1]);
  expect(request.pending).toBe(false);
});

test("a known refusal releases the operation and an empty retry is refused", async () => {
  const keys: string[] = [];
  const request = new RetainedRequest<string>();
  const refused = new ServerError({detail: "stale", suggestion: "review", status: 409, code: "STALE", requestId: null});
  await Effect.runPromise(Effect.flip(request.run(IDENTITY, (key) => { keys.push(key); return Effect.fail(refused); })));
  expect(request.pending).toBe(false);
  expect((await Effect.runPromise(Effect.flip(request.retry()))).detail).toContain("no uncertain operation");
  expect(await Effect.runPromise(request.run({message: "new"}, (key) => { keys.push(key); return Effect.succeed("new"); }))).toBe("new");
  expect(keys[0]).not.toBe(keys[1]);
});

test("a late completion from a cleared request cannot erase a newer unknown operation", async () => {
  const request = new RetainedRequest<string>();
  const first = Promise.withResolvers<string>();
  const running = Effect.runPromise(request.run(IDENTITY, () => Effect.tryPromise({try: () => first.promise, catch: () => network})));
  request.clear();
  await Effect.runPromise(Effect.flip(request.run({message: "next"}, () => Effect.fail(network))));
  first.resolve("old answer");
  await running;
  expect(request.pending).toBe(true);
  expect(await Effect.runPromise(Effect.flip(request.retry()))).toBe(network);
});

test("clearing an invocation disposes of an uncertain private request", async () => {
  const request = new RetainedRequest<string>();
  await Effect.runPromise(Effect.flip(request.run(IDENTITY, () => Effect.fail(network))));
  request.clear();
  expect(request.pending).toBe(false);
  expect((await Effect.runPromise(Effect.flip(request.retry()))).detail).toContain("no uncertain operation");
});

test.each([408, 500, 503])("HTTP %i retains the original identity for reconciliation", async (status) => {
  const request = new RetainedRequest<string>();
  const keys: string[] = [];
  const error = new ServerError({detail: "unknown", suggestion: "retry", status, code: "UNKNOWN", requestId: null});
  await Effect.runPromise(Effect.flip(request.run(IDENTITY, (key) => Effect.suspend(() => {
    keys.push(key); return keys.length === 1 ? Effect.fail(error) : Effect.succeed("confirmed");
  }))));
  expect(request.pending).toBe(true);
  expect(await Effect.runPromise(request.retry())).toBe("confirmed");
  expect(keys[0]).toBe(keys[1]);
});


test("a supplied recovery identity is used once and concurrent retries cannot resend", async () => {
  const key = "0199a801-1000-7000-8000-000000000009";
  const request = new RetainedRequest<string>(key);
  const answer = Promise.withResolvers<string>();
  const keys: string[] = [];
  const running = Effect.runPromise(request.run(IDENTITY, (operation) => {
    keys.push(operation); return Effect.tryPromise({try: () => answer.promise, catch: () => network});
  }));
  const rejected = await Effect.runPromise(Effect.flip(request.retry()));
  expect(rejected.detail).toContain("still in progress"); expect(keys).toEqual([key]);
  answer.resolve("confirmed"); expect(await running).toBe("confirmed");
  await Effect.runPromise(request.run({message: "new message"}, (operation) => {keys.push(operation); return Effect.succeed("new");}));
  expect(keys[1]).not.toBe(key); expect(request.operationId).toBeUndefined();
});
