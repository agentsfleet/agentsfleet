import { z } from "zod";
import type { Definition, Draft } from "./draft-types";
import type { DrawerEdit } from "./draft-state";
import { SetupUnavailable } from "./draft-request";

const encoder = new TextEncoder();
const RUNTIME_EXTENSION = "x-agentsfleet";
const RuntimeConfigSchema = z.record(z.string(), z.unknown());
const DASHBOARD_TOOLS = ["exec_command", "write_stdin", "file_read", "file_write", "file_edit", "web_fetch"] as const;
type ConfigOp = {op: "remove"; path: string} | {op: "add" | "replace"; path: string; value: unknown};

export function dashboardToolPatch(draft: Draft): {config_ops: ConfigOp[]} | undefined {
  if (draft.linked_fleet_id || !draft.review || draft.review.effective.source_revision) return;
  const runtime = RuntimeConfigSchema.safeParse(draft.candidate?.definition.config[RUNTIME_EXTENSION]);
  if (!runtime.success || !Array.isArray(runtime.data.tools) || runtime.data.tools.length) return;
  return {config_ops: [{op: "replace", path: `/config/${RUNTIME_EXTENSION}/tools`, value: DASHBOARD_TOOLS}]};
}

export function createdDrawerPatch(created: Draft, input: DrawerEdit) {
  if (!created.candidate) throw new SetupUnavailable("The selected source has no editable setup.", 409);
  if (JSON.stringify(created.candidate.definition) !== JSON.stringify(input.definition) && !drawerMatches(created.candidate.definition, input)) {
    return drawerPatch(created.candidate.definition, input);
  }
  if (input.instanceName !== undefined && created.review?.effective.instance_name !== input.instanceName
    || input.runtimeModel !== undefined && created.review?.effective.runtime_model_ref !== input.runtimeModel) {
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
  const config_ops = configChanges(prior.config, input.definition.config, "/config");
  if (prior.description !== input.definition.description) {
    config_ops.push({op: "replace", path: "/description", value: input.definition.description});
  }
  const instruction_edits = prior.instructions_md === input.definition.instructions_md ? []
    : [{start_byte: 0, end_byte: encoder.encode(prior.instructions_md).byteLength, text: input.definition.instructions_md}];
  if (!config_ops.length && !instruction_edits.length && input.instanceName === undefined && input.runtimeModel === undefined) return;
  return {instance_name: input.instanceName, ...(input.runtimeModel ? {runtime_model_ref: input.runtimeModel} : {}), config_ops,
    instruction_edits};
}

function configChanges(before: Definition["config"], after: Definition["config"], root: string): ConfigOp[] {
  return [...new Set([...Object.keys(before), ...Object.keys(after)])].flatMap<ConfigOp>(key => {
    const path = `${root}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`;
    if (!Object.hasOwn(after, key)) return [{op: "remove", path}];
    if (JSON.stringify(before[key]) === JSON.stringify(after[key])) return [];
    if (root === "/config" && key === RUNTIME_EXTENSION) {
      const original = RuntimeConfigSchema.safeParse(before[key]);
      const edited = RuntimeConfigSchema.safeParse(after[key]);
      if (original.success && edited.success) return configChanges(original.data, edited.data, path);
    }
    return [{op: Object.hasOwn(before, key) ? "replace" : "add", path, value: after[key]}];
  });
}
