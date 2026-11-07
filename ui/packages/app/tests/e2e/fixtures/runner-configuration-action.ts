import { z } from "zod";
import type { RunnerEdit } from "@/lib/api/runners";
import { JEV_MODEL } from "@/tests/helpers/catalogue-fixtures";

const SAVE_TIMEOUT_MS = 5_000;
const saveResult = z.discriminatedUnion("ok", [
  z.object({ok: z.literal(true), data: z.object({id: z.string()})}),
  z.object({ok: z.literal(false), error: z.string(), errorCode: z.string().optional()}),
]);

// The component fixture replaces only the server action's HTTP boundary.
export async function updateRunnerAction(runnerId: string, edit: RunnerEdit) {
  const response = await fetch(`/runner/${encodeURIComponent(runnerId)}`, {
    method: "PATCH", headers: {"content-type": "application/json"},
    body: JSON.stringify(edit), signal: AbortSignal.timeout(SAVE_TIMEOUT_MS),
  });
  return saveResult.parse(await response.json());
}

export async function listAdminModelsAction() {
  return {ok: true as const, data: {models: [JEV_MODEL]}};
}
