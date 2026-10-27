import { z } from "zod";
import type { Definition, Draft } from "./draft-types";
import type { DrawerEdit } from "./draft-state";
import { SetupUnavailable } from "./draft-request";

const encoder = new TextEncoder();
const ROOTS = ["config"] as const;
const RuntimeConfigSchema = z.record(z.string(), z.unknown());
type ConfigOp = {op: "remove"; path: string} | {op: "add" | "replace"; path: string; value: unknown};

export function createdDrawerPatch(created: Draft, input: DrawerEdit) {
  if (!created.candidate) throw new SetupUnavailable("The selected source has no editable setup.", 409);
  if (JSON.stringify(created.candidate.definition) !== JSON.stringify(input.definition) && !drawerMatches(created.candidate.definition, input)) {
    return drawerPatch(created.candidate.definition, input);
  }
  if (created.review?.effective.instance_name !== input.instanceName
    || input.runtimeModel !== undefined && created.review.effective.runtime_model_ref !== input.runtimeModel) {
    return {instance_name: input.instanceName, ...(input.runtimeModel ? {runtime_model_ref: input.runtimeModel} : {})};
  }
  return undefined;
}

/** A replayed creation reads the current candidate after a lost native-edit response. */
export function drawerMatches(prior: Definition, input: DrawerEdit): boolean {
  const expected = input.runtimeModel ? {...input.definition, config: {...input.definition.config,
    "x-agentsfleet": {...RuntimeConfigSchema.parse(input.definition.config["x-agentsfleet"]), model: input.runtimeModel}}} : input.definition;
  return JSON.stringify(prior) === JSON.stringify(expected);
}

export function drawerPatch(prior: Definition, input: DrawerEdit) {
  const config_ops = ROOTS.flatMap((root) => {
    const before = prior[root];
    const after = input.definition[root];
    return [...new Set([...Object.keys(before), ...Object.keys(after)])].flatMap<ConfigOp>((key) => {
      const path = `/${root}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`;
      if (!Object.hasOwn(after, key)) return [{op: "remove", path}];
      if (JSON.stringify(before[key]) === JSON.stringify(after[key])) return [];
      return [{op: Object.hasOwn(before, key) ? "replace" : "add", path, value: after[key]}];
    });
  });
  if (prior.description !== input.definition.description) {
    config_ops.push({op: "replace", path: "/description", value: input.definition.description});
  }
  return {instance_name: input.instanceName, ...(input.runtimeModel ? {runtime_model_ref: input.runtimeModel} : {}), config_ops,
    instruction_edits: [{start_byte: 0, end_byte: encoder.encode(prior.instructions_md).byteLength, text: input.definition.instructions_md}]};
}
