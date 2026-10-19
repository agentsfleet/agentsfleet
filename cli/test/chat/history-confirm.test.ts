import { expect, test } from "bun:test";
import { Effect } from "effect";
import { chatDeleteEffect } from "../../src/commands/chat/history.ts";
import { mainLayerFor } from "../../src/runtime/main-layer.ts";
import { Input } from "../../src/services/input.ts";
import { Stdin } from "../../src/services/stdin.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { DRAFT_ID, WORKSPACE, withApi } from "./fixture.ts";

test.each(["y", " YES ", "no", "", null])("interactive delete only removes the chat after an affirmative answer: %s", async answer => {
  const calls: string[] = [];
  const prompts: string[] = [];
  await withApi(request => {
    calls.push(`${request.method} ${new URL(request.url).pathname}`);
    return new Response(null, {status: 204});
  }, async (_transport, origin) => withAuthedStateDir({workspaceId: WORKSPACE, apiUrl: origin}, async () => {
    const stdout = bufferStream(); const stderr = bufferStream();
    await Effect.runPromise(chatDeleteEffect(DRAFT_ID, {yes: false}).pipe(
      Effect.provideService(Input, {readLine: prompt => Effect.sync(() => {prompts.push(prompt); return answer;})}),
      Effect.provideService(Stdin, {isTTY: true, readToEnd: Effect.die("Unexpected bulk input")}),
      Effect.provide(mainLayerFor({config: {apiUrl: origin, telemetryPosthogKey: "", telemetryPosthogHost: origin},
        env: cliEnv(), streams: {stdout: stdout.stream, stderr: stderr.stream}})),
    ));
    const confirmed = answer?.trim().toLowerCase() === "y" || answer?.trim().toLowerCase() === "yes";
    expect(calls).toEqual(confirmed ? [`DELETE /v1/workspaces/${WORKSPACE}/drafts/${DRAFT_ID}`] : []);
    expect(prompts).toHaveLength(1);
    expect(prompts[0]).toContain("Saved fleets keep running");
    expect(stdout.read()).toContain(confirmed ? "Deleted setup chat" : "deletion cancelled");
  }));
});
