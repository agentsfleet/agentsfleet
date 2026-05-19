// Live Browser layer. Wraps the non-Effect openUrl in lib/browser.ts
// (which owns BROWSER=false / DISPLAY / SSH / WSL detection + spawn).
// Tag and shape live in browser.service.ts.

import { Effect, Layer } from "effect";
import { Browser } from "./browser.service.ts";
import { openUrl as openUrlRaw } from "../lib/browser.ts";

export const browserLayer: Layer.Layer<Browser> = Layer.succeed(
  Browser,
  Browser.of({
    open: (url: string) => Effect.promise(() => openUrlRaw(url)),
  }),
);
