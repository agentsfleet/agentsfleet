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

/** The billing segment shares one catalogue walk across its server render. */
export const modelLabelsCached = cache(readModelLabels);
