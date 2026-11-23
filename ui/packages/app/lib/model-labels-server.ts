import type { LibraryModel } from "@/lib/api/model-library-types";
import { cache } from "react";
import { getModelLibrary } from "@/lib/api/model_library";
import { modelLabelsOf, type ModelLabels } from "@/lib/model-labels";

/** The server's page ceiling (`MAX_LIMIT` in af_core::paging), so the walk takes the fewest round trips. */
const CATALOGUE_PAGE_LIMIT = 100;

/** The complete catalogue, advanced models included, reduced to unambiguous labels. */
async function readModelLabels(token: string): Promise<ModelLabels> {
  const models: LibraryModel[] = [];
  let cursor: string | null = null;
  // Half a catalogue could label an identifier another page disputes, so the
  // walk reads to the end; only a repeated cursor, which would replay forever, fails it.
  const followed = new Set<string>();
  for (;;) {
    const library = await getModelLibrary(token, true, cursor, CATALOGUE_PAGE_LIMIT);
    models.push(...library.models);
    cursor = library.next_cursor;
    if (cursor === null) return modelLabelsOf(models);
    if (followed.has(cursor)) throw new Error(`The model catalogue repeated page cursor ${cursor}`);
    followed.add(cursor);
  }
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
