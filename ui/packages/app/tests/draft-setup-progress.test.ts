// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, DRAFT_ID, FLEET, PUBLICATION, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it("reports Saving until the accepted fleet response can be read", async () => {
  const client = await fixture.ready(true);
  const gate = Promise.withResolvers<Response>();
  fixture.override = ({path, method}) => method === "POST" && path.endsWith("/fleets") ? gate.promise : undefined;
  const saving = client.save();
  await vi.waitFor(() => expect(fixture.mutations()).toHaveLength(1));
  expect(client.getSnapshot()).toMatchObject({busy: true, setupAction: "saving", fleet: null});
  gate.resolve(response(FLEET, 201)); await saving;
  expect(client.getSnapshot()).toMatchObject({busy: false, pending: false, setupAction: null, savedSetupRevision: "1", fleet: {id: FLEET.id}});
});

it("reports Publishing until the native publication receipt is confirmed", async () => {
  const client = await fixture.ready(true);
  const gate = Promise.withResolvers<Response>();
  fixture.override = ({path, method}) => method === "POST" && path.endsWith("/fleet-libraries") ? gate.promise : undefined;
  const publishing = client.publish("Reviewed setup", false);
  await vi.waitFor(() => expect(fixture.mutations()).toHaveLength(1));
  expect(client.getSnapshot()).toMatchObject({busy: true, setupAction: "publishing"});
  expect(client.getSnapshot().draft?.publication).toBeUndefined();
  fixture.current = {...DRAFT, publication: {...PUBLICATION, canonical_digest: "candidate-one"}};
  gate.resolve(response(PUBLICATION, 201)); await publishing;
  expect(client.getSnapshot()).toMatchObject({busy: false, pending: false, setupAction: null, notice: "Setup published.", draft: {publication: {revision_id: PUBLICATION.revision_id}}});
});

it("keeps the original pending action and operation key after a lost publication response", async () => {
  const client = await fixture.ready(true);
  fixture.override = ({path, method}) => method === "POST" && path.endsWith("/fleet-libraries") ? response({user_message: "Unavailable"}, 503) : undefined;
  await expect(client.publish("Reviewed setup", false)).rejects.toMatchObject({status: 503});
  expect(client.getSnapshot()).toMatchObject({busy: false, pending: true, setupAction: "publishing"});
  const key = fixture.mutations()[0]?.headers.get("Idempotency-Key");
  await expect(client.save()).rejects.toMatchObject({status: 409});
  expect(client.getSnapshot().setupAction).toBe("publishing");
  expect(fixture.mutations()).toHaveLength(1);
  fixture.override = () => undefined;
  fixture.current = {...DRAFT, publication: {...PUBLICATION, canonical_digest: "candidate-one"}};
  await client.retry();
  expect(fixture.mutations()[1]?.headers.get("Idempotency-Key")).toBe(key);
  expect(client.getSnapshot()).toMatchObject({pending: false, setupAction: null, error: null});
});

it("reports Reading setup during a real setup request without writing anything", async () => {
  const client = await fixture.ready(true);
  const gate = Promise.withResolvers<Response>();
  fixture.calls.length = 0;
  fixture.override = ({path}) => path.endsWith(`/drafts/${DRAFT_ID}`) ? gate.promise : undefined;
  const reading = client.refreshSetup();
  expect(client.getSnapshot()).toMatchObject({busy: true, setupAction: "reading"});
  gate.resolve(response(DRAFT)); await reading;
  expect(fixture.mutations()).toHaveLength(0);
  expect(client.getSnapshot()).toMatchObject({busy: false, setupAction: null});
});

it.each(["save", "apply", "publish"] as const)("clears %s progress when the displayed source is no longer available", async action => {
  const client = await fixture.ready(true);
  fixture.current = {...DRAFT, candidate: undefined, review: undefined};
  const request = action === "save" ? client.save() : action === "apply" ? client.apply() : client.publish("Reviewed setup", false);
  await expect(request).rejects.toMatchObject({status: 409});
  expect(client.getSnapshot()).toMatchObject({busy: false, pending: false, setupAction: null});
  expect(fixture.mutations()).toHaveLength(0);
});
