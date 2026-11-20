import type { Source } from "./draft-types";
import { z } from "zod";
import { DefinitionSchema } from "./draft-types";
import { SOURCE_KIND } from "./draft-vocabulary";
import { setupRequest, SetupUnavailable } from "./draft-request";

const FleetSeedSchema = z.object({id: z.uuidv7(), definition: DefinitionSchema, name: z.string(), setup_revision: z.string()});
export async function sourcePreview(base: string, source: Source, signal: AbortSignal) {
  if (source.kind === SOURCE_KIND.blank) return null;
  if (source.kind === SOURCE_KIND.definition) return {definition: source.definition, instanceName: String(source.definition.config.name), revision: "inline"};
  const fleet = await setupRequest({path: `${base}/fleets/${encodeURIComponent(source.fleet_id)}`}, FleetSeedSchema, signal);
  if (fleet.id !== source.fleet_id || fleet.setup_revision !== source.setup_revision) throw new SetupUnavailable("This fleet changed. Open Edit setup from its current setup.", 409);
  return {definition: fleet.definition, instanceName: fleet.name, revision: source.setup_revision, setupRevision: source.setup_revision};
}
