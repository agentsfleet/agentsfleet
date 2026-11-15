import type { Source } from "./draft-types";
import { z } from "zod";
import { DefinitionSchema } from "./draft-types";
import { SOURCE_KIND } from "./draft-vocabulary";
import { PluginSchema } from "./draft-plugin";
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
  const plugin = await setupRequest({path: `${base}/plugins/${encodeURIComponent(source.plugin_id)}?${query}`}, PluginSchema, signal);
  if (plugin.card.id !== source.plugin_id || plugin.head.plugin_id !== source.plugin_id || plugin.head.id !== source.revision_id) throw new SetupUnavailable("The selected Plugin revision could not be confirmed.", 409);
  return {definition: plugin.head.definition, instanceName: plugin.review.effective.instance_name, revision: plugin.head.id,
    sourceLabel: `${plugin.card.name} · v${plugin.head.revision}`};
}
