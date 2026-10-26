import { expect, it } from "vitest";
import { drawerMatches, drawerPatch } from "@/lib/api/draft-edits";
import { REVIEW } from "./helpers/draft-fixtures";

it("recognizes a completed native edit while retaining all other setup content", () => {
  const definition = REVIEW.effective.definition;
  expect(drawerMatches(definition, {definition, instanceName: "one"})).toBe(true);
  const edited = {...definition, instructions_md: "Unrelated change"};
  expect(drawerMatches(edited, {definition, instanceName: "one"})).toBe(false);
});

it("patches removed, added and changed keys while escaping pointers and preserving byte offsets", () => {
  const prior = {...REVIEW.effective.definition, instructions_md: "你好", config: {keep: true, remove: 1, "a~/b": 2}, description: "Old description"};
  const definition = {...prior, instructions_md: "New", config: {keep: true, "a~/b": 3, added: false}, description: "New description"};
  expect(drawerPatch(prior, {definition, instanceName: "one", runtimeModel: "model:one"})).toEqual({
    instance_name: "one", runtime_model_ref: "model:one",
    config_ops: [{op: "remove", path: "/config/remove"}, {op: "replace", path: "/config/a~0~1b", value: 3},
      {op: "add", path: "/config/added", value: false}, {op: "replace", path: "/description", value: "New description"}],
    instruction_edits: [{start_byte: 0, end_byte: 6, text: "New"}],
  });
  expect(drawerPatch(prior, {definition: prior, instanceName: "one"})).toMatchObject({config_ops: []});
});
