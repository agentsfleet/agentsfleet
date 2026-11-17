import {Effect} from "effect";
import {Output, OUTPUT_FORMAT} from "../../services/output.ts";
import {PET_DISPLAY, PET_NAME, PET_PREVIEW_MS} from "../../constants/pet.ts";
import {SIGINT, SIGTERM} from "../../constants/signals.ts";
import {InterruptedError} from "../../errors/index.ts";

export const petPreview = Effect.gen(function* () {
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) {
    return yield* output.printJson({name: PET_NAME});
  }
  yield* Effect.scoped(Effect.gen(function* () {
    yield* output.progress(PET_NAME, PET_DISPLAY.preview);
    const completed = yield* waitForPreview();
    if (!completed) return yield* Effect.fail(new InterruptedError({detail: "pet preview canceled", suggestion: "run `agentsfleet pet` to watch Orbit again"}));
  }));
  yield* output.info("Orbit appears while a Platform request is running.");
});

function waitForPreview() {
  return Effect.promise(owner => new Promise<boolean>(resolve => {
    const finish = (completed: boolean): void => {
      clearTimeout(timer);
      process.removeListener(SIGINT, cancel); process.removeListener(SIGTERM, cancel);
      owner.removeEventListener("abort", cancel);
      resolve(completed);
    };
    const cancel = (): void => finish(false);
    const timer = setTimeout(() => finish(true), PET_PREVIEW_MS);
    process.once(SIGINT, cancel); process.once(SIGTERM, cancel);
    owner.addEventListener("abort", cancel, {once: true});
    if (owner.aborted) cancel();
  }));
}
