import { z } from "zod";
import type { Definition, Review } from "./api/draft-types";

const AmountSchema = z.string().regex(/^[1-9][0-9]*$/);
export const SetupRequirementsSchema = z.object({
  credentials: z.array(z.string()), repositories: z.array(z.string()), tools: z.array(z.string()),
  network: z.object({http_egress: z.enum(["restricted", "public"]).optional(),
    package_manager_domains: z.boolean().optional(), allow: z.array(z.string()).optional(), read_only: z.boolean().optional()}).optional(),
  budget: z.object({daily_nanos: AmountSchema, monthly_nanos: AmountSchema.optional()}),
});
const SavedConfigSchema = SetupRequirementsSchema.extend({
  credentials: z.array(z.string()).default([]), repositories: z.array(z.string()).default([]), model: z.string().optional(),
});

export type SetupNetworkChange = {http_egress: "public" | "restricted"} | {package_manager_domains: boolean};

/** Review requirements are authoritative; saved definitions omit empty lists. */
export function setupRequirements(definition?: Definition, requirements?: Review["requirements"]) {
  const result = requirements === undefined ? SavedConfigSchema.safeParse(definition?.config["x-agentsfleet"])
    : SetupRequirementsSchema.safeParse(requirements);
  return result.success ? result.data : null;
}
