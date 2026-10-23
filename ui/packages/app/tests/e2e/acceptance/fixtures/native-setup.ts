import type { Definition, Draft, Fleet, Source } from "@/lib/api/draft-types";
import type { ClientHandle } from "./api-client";
import { v7 as uuidv7 } from "uuid";
import { DraftSchema, FleetSchema, ModelsSchema, PublishedSchema, SETUP_HEADERS, SOURCE_KIND } from "@/lib/api/draft-types";
import { clientFor } from "./api-client";
import { FIXTURE_KEY } from "./constants";
import { fixtureDefinition } from "./definition";

const PLATFORM_LIBRARIES = "/v1/admin/fleet-libraries";
const MODEL_PAGE_LIMIT = 100;
const MODEL_PAGE_MAX = 100;
const PLATFORM_DEFAULT_REF = "platform-default";

export async function createFixtureDraft(handle: ClientHandle, workspace: string, definition: Definition): Promise<Draft> {
  return createDraft(handle, workspace, { kind: SOURCE_KIND.definition, definition });
}

export async function readFixtureFleet(handle: ClientHandle, workspace: string, fleetId: string): Promise<Fleet> {
  return FleetSchema.parse(await clientFor(handle).get<unknown>(`/v1/workspaces/${workspace}/fleets/${encodeURIComponent(fleetId)}`));
}

export async function deleteFixtureDraft(handle: ClientHandle, workspace: string, draftId: string): Promise<void> {
  await clientFor(handle).delete(`/v1/workspaces/${workspace}/drafts/${encodeURIComponent(draftId)}`);
}

export async function publishFixtureLibrary(handle: ClientHandle, workspace: string, definition: Definition) {
  const draft = await createFixtureDraft(handle, workspace, definition);
  if (!draft.candidate) throw new Error("Fixture draft has no candidate");
  return PublishedSchema.parse(await clientFor(handle).post<unknown>(`/v1/workspaces/${workspace}/fleet-libraries`, {
    draft_candidate_id: draft.candidate.id, label: definition.config.name,
  }, { [SETUP_HEADERS.key]: uuidv7() }));
}

export async function saveFixtureFleet(handle: ClientHandle, workspace: string, definition: Definition,
  name: string): Promise<Fleet> {
  const draft = await createFixtureDraft(handle, workspace, { ...definition, config: { ...definition.config, name } });
  if (!draft.candidate || !draft.review) throw new Error("Fixture draft is not reviewable");
  return FleetSchema.parse(await clientFor(handle).post<unknown>(`/v1/workspaces/${workspace}/fleets`, {
    source: { kind: "draft", draft_candidate_id: draft.candidate.id }, name,
    review_digest: draft.review.review_digest, start: { kind: "none" },
  }, { [SETUP_HEADERS.key]: uuidv7() }));
}

export async function stageFixturePlatform(handle: ClientHandle, workspace: string, definition: Definition, slug: string) {
  const draft = await createFixtureDraft(handle, workspace, definition);
  if (!draft.candidate) throw new Error("Fixture draft has no candidate");
  return PublishedSchema.parse(await clientFor(handle).post<unknown>(
    "/v1/admin/fleet-libraries", { workspace_id: workspace, draft_candidate_id: draft.candidate.id,
      label: slug }, { [SETUP_HEADERS.key]: uuidv7() }));
}

type PlatformRows = { entries: { card: { id: string; etag: string }; published: boolean }[] };

/** Stage a platform entry from the operator's `workspace`, run `body`, then withdraw and delete whatever is left. */
export async function withStagedPlatformEntry(workspace: string, slug: string,
  body: (libraryId: string) => Promise<void>): Promise<void> {
  const staged = await stageFixturePlatform(FIXTURE_KEY.operator, workspace, fixtureDefinition(slug), slug);
  try {
    await body(staged.library_id);
  } finally {
    await removePlatformEntry(staged.library_id);
  }
}

async function removePlatformEntry(id: string): Promise<void> {
  const client = clientFor(FIXTURE_KEY.operator);
  const path = `${PLATFORM_LIBRARIES}/${encodeURIComponent(id)}`;
  const find = async () => (await client.get<PlatformRows>(PLATFORM_LIBRARIES)).entries.find((item) => item.card.id === id);
  let current = await find();
  if (!current) return;
  if (current.published) {
    // A published entry refuses deletion; withdraw it first and delete under its fresh tag.
    await client.patch(path, { published: false }, { [SETUP_HEADERS.match]: current.card.etag });
    current = await find();
    if (!current) throw new Error("Withdrawn platform entry disappeared before cleanup");
  }
  await client.delete(path, { [SETUP_HEADERS.match]: current.card.etag });
}

async function createDraft(handle: ClientHandle, workspace: string, source: Source): Promise<Draft> {
  const model = await authoringModelRef(handle, workspace);
  return DraftSchema.parse(await clientFor(handle).post<unknown>(`/v1/workspaces/${workspace}/drafts`, {
    source, authoring_model_ref: model, operation_id: uuidv7(),
  }));
}

/** Prefer the live platform model over entries other tests create with dummy keys. */
export async function authoringModelRef(handle: ClientHandle, workspace: string): Promise<string> {
  const client = clientFor(handle);
  let cursor: string | null = null;
  let fallback: string | undefined;
  for (let page = 0; page < MODEL_PAGE_MAX; page++) {
    const query = new URLSearchParams({ limit: String(MODEL_PAGE_LIMIT) });
    if (cursor) query.set("starting_after", cursor);
    const models = ModelsSchema.parse(await client.get<unknown>(`/v1/workspaces/${workspace}/models?${query}`));
    const eligible = models.items.filter((entry) => entry.eligibility.authoring);
    const model = eligible.find((entry) => entry.ref === PLATFORM_DEFAULT_REF);
    if (model) return model.ref;
    fallback ??= eligible[0]?.ref;
    if (!models.next_cursor || models.next_cursor === cursor) break;
    cursor = models.next_cursor;
  }
  if (fallback) return fallback;
  throw new Error("No eligible workspace authoring model for acceptance fixture");
}
