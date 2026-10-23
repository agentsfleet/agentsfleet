import type { Source } from "./draft-types";
import { z } from "zod";
import { DefinitionSchema } from "./draft-types";
import { SOURCE_KIND } from "./draft-vocabulary";
import { LibrarySchema } from "./draft-library";
import { setupRequest, SetupUnavailable } from "./draft-request";

const FleetSeedSchema = z.object({id: z.uuidv7(), definition: DefinitionSchema, name: z.string(), setup_revision: z.string()});

export async function sourcePreview(base: string, source: Source, signal: AbortSignal) {
  if (source.kind === SOURCE_KIND.blank) return null;
  if (source.kind === SOURCE_KIND.definition) return {definition: source.definition, instanceName: String(source.definition.config.name), revision: "inline"};
  if (source.kind === SOURCE_KIND.fleet) {
    const fleet = await setupRequest({path: `${base}/fleets/${encodeURIComponent(source.fleet_id)}`}, FleetSeedSchema, signal);
    if (fleet.id !== source.fleet_id || fleet.setup_revision !== source.setup_revision) throw new SetupUnavailable("This fleet changed. Open Edit setup from its current setup.", 409);
    return {definition: fleet.definition, instanceName: fleet.name, revision: source.setup_revision, setupRevision: source.setup_revision};
  }
  const query = new URLSearchParams({revision_id: source.revision_id});
  const library = await setupRequest({path: `${base}/fleet-libraries/${encodeURIComponent(source.library_id)}?${query}`}, LibrarySchema, signal);
  if (library.card.id !== source.library_id || library.head.library_id !== source.library_id || library.head.id !== source.revision_id) throw new SetupUnavailable("The selected Library revision could not be confirmed.", 409);
  return {definition: library.head.definition, instanceName: library.review.effective.instance_name, revision: library.head.id};
}
