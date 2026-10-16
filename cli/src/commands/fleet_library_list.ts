import type { LibraryPageOptions } from "./fleet-library-read.ts";
import { libraryEffectFromOptions } from "./fleet_library.ts";

export const libraryListEffectFromOptions = (options: LibraryPageOptions) =>
  libraryEffectFromOptions(options, true);
