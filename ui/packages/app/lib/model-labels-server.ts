import type { LibraryModel } from "@/lib/api/model-library-types";
import { cache } from "react";
import { getModelLibrary } from "@/lib/api/model_library";
import { modelLabelsOf, type ModelLabels } from "@/lib/model-labels";

/** A catalogue this deep is a fault, and half a catalogue could label an identifier another page disputes. */
const MAX_PAGES = 40;

/** The complete catalogue, advanced models included, reduced to unambiguous labels. */
async function readModelLabels(token: string): Promise<ModelLabels> {
  const models: LibraryModel[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const library = await getModelLibrary(token, true, cursor);
    models.push(...library.models);
    cursor = library.next_cursor;
    if (cursor === null) return modelLabelsOf(models);
  }
  throw new Error("The model catalogue did not end within its page bound");
}

/**
 * Model display names, read once per document load.
 *
 * The dashboard layout is the only caller, and Next does not re-render a
 * layout on client navigation, so a sign-in or a refresh reads the catalogue
 * and the browser keeps the answer until the next one. `cache()` shares that
 * read within one server render, as the layout's other shell reads do.
 */
export const modelLabelsCached = cache(readModelLabels);
