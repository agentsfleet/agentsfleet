// @vitest-environment node
import { afterEach, beforeEach, expect, it } from "vitest";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { MODEL_REF, REVIEW } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it.each(["same-name", "renamed"])("includes a runtime-only change in the first native edit (%s)", async (name) => {
  const client = await fixture.ready();
  const instanceName = name === "same-name" ? REVIEW.effective.instance_name : "Support triage";
  await client.edit({definition: REVIEW.effective.definition, instanceName, runtimeModel: "model:runtime"});
  expect(fixture.mutations()).toHaveLength(2);
  expect(fixture.mutations()[1]?.body).toEqual({instance_name: instanceName, runtime_model_ref: "model:runtime"});
});

it("creates no redundant native edit when the requested model and name already match", async () => {
  const client = await fixture.ready();
  await client.edit({definition: REVIEW.effective.definition, instanceName: REVIEW.effective.instance_name, runtimeModel: MODEL_REF});
  expect(fixture.mutations()).toHaveLength(1);
});

it("returns the refreshed editable setup and rejects a missing source without a mutation", async () => {
  const client = await fixture.ready();
  await expect(client.refreshSetup()).rejects.toMatchObject({status: 409, message: "There is no setup to refresh yet."});
  expect(client.getSnapshot()).toMatchObject({preview: null, busy: false, pending: false});
  client.fresh({kind: "definition", definition: REVIEW.effective.definition});
  expect(await client.refreshSetup()).toMatchObject({definition: REVIEW.effective.definition, instanceName: "repairer"});
  expect(fixture.mutations()).toEqual([]);
});
