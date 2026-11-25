import { Effect } from "effect";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { Stdin } from "../../services/stdin.ts";
import { PET_NAME } from "../../constants/pet.ts";
import { InterruptedError, UnexpectedError } from "../../errors/index.ts";
import { isOrbitInput, playOrbit } from "./play.ts";

const PLAY = "Run `agentsfleet pet` in an interactive terminal to play Orbit.";
const RETRY = "run `agentsfleet pet` to play again";

export const petPreview = Effect.gen(function* () {
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) {
    return yield* output.printJson({name: PET_NAME});
  }
  const input = (yield* Stdin).stream;
  const screen = output.screen;
  if (!screen || !isOrbitInput(input)) return yield* output.info(PLAY);
  const outcome = yield* Effect.tryPromise({
    try: signal => playOrbit(input, screen, signal),
    catch: () => new UnexpectedError({detail: "Could not draw Orbit", suggestion: RETRY}),
  });
  switch (outcome) {
    case "quit": return;
    case "small": return yield* output.info("Make your terminal larger to play Orbit.");
    case "resized": return yield* output.info("Terminal resized. Run `agentsfleet pet` to play again.");
    case "interrupted": return yield* Effect.fail(new InterruptedError({detail: "Orbit interrupted", suggestion: RETRY}));
  }
});
