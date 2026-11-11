import {Effect} from "effect";
import {Output, OUTPUT_FORMAT} from "../../services/output.ts";
import {PET_NAME, PET_PREVIEW_MS} from "../../constants/pet.ts";

export const petPreview = Effect.gen(function* () {
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) {
    return yield* output.printJson({name: PET_NAME});
  }
  yield* Effect.scoped(Effect.gen(function* () {
    yield* output.progress(PET_NAME);
    yield* Effect.sleep(PET_PREVIEW_MS);
  }));
  yield* output.info("Scout appears while a Platform request is running.");
});
