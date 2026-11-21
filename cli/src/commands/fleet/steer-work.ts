import { Effect, Redacted } from "effect";
import { wsFleetTurnPath, wsFleetPath } from "../../lib/api-paths.ts";
import { isRecord, isString } from "../../lib/guards.ts";
import { isValidId } from "../../lib/id.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { TaskDetailSchema, decodeTask } from "./task-read.ts";
import { renderRepository } from "./tasks.ts";

/** Read the admitted turn's saved task identities without sending more work. */
export function readSteeredWork(workspace: string, fleet: string, turn: string,
  token: Redacted.Redacted<string>, signal?: AbortSignal) {
  return Effect.gen(function* () {
    const output = yield* Output;
    if (output.format !== OUTPUT_FORMAT.text || signal?.aborted) return;
    const http = yield* HttpClient;
    const read = Effect.gen(function* () {
      const saved = yield* http.request<unknown>({path: wsFleetTurnPath(workspace, fleet, turn), token, signal,
        timeoutMs: 5000, retry: {maxAttempts: 1}});
      if (!isRecord(saved) || saved["turn_id"] !== turn) return;
      const history = saved["task_activity"];
      if (!isRecord(history) || !Array.isArray(history["items"])) return;
      let task: string | undefined;
      for (const item of history["items"].slice(-128).reverse()) {
        if (!isRecord(item) || !isRecord(item["progress"])) continue;
        const id = item["progress"]["task_id"];
        if (isString(id) && isValidId(id)) { task = id; break; }
      }
      // A turn's latest selected work is the relevant saved dependency.
      if (!task || signal?.aborted) return;
      const detail = yield* http.request<unknown>({path: `${wsFleetPath(workspace, fleet)}/tasks/${encodeURIComponent(task)}`,
        token, signal, timeoutMs: 5000, retry: {maxAttempts: 1}}).pipe(Effect.flatMap(value => decodeTask(TaskDetailSchema, value)));
      yield* renderRepository(detail);
    });
    yield* read.pipe(Effect.catch(() => signal?.aborted ? Effect.void : output.info("Saved work details could not be refreshed. The admitted request was not resent.")));
  });
}
