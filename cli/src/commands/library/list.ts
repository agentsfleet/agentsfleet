import type { LibraryPageOptions } from "./read.ts";
import { libraryEffectFromOptions } from "./library.ts";

export const libraryListEffectFromOptions = (options: LibraryPageOptions) =>
  libraryEffectFromOptions(options, true);
