import type { z } from "zod";
import type { StagePlatform } from "./platform-library-types";
import type { Draft } from "./draft-types";
import type { SetupRequest } from "./draft-request";
import { z as schema } from "zod";
import { PublishedSchema, SETUP_HEADERS } from "./draft-types";
import { REQUEST_METHOD, SetupUnavailable } from "./draft-request";

export type PlatformPublisher = (body: StagePlatform, operation: string) => Promise<unknown>;
type Request = <T>(request: SetupRequest, parser: z.ZodType<T>) => Promise<T>;
type ReadyDraft = Draft & {candidate: NonNullable<Draft["candidate"]>};

/** Library writes retain the operation identity supplied by the private chat owner. */
export class DraftPublication {
  constructor(private readonly base: string, private readonly workspace: string,
    private readonly request: Request, private readonly platform?: PlatformPublisher) {}

  get canStagePlatform(): boolean { return this.platform !== undefined; }

  async prepare(draft: ReadyDraft, label: string, append: boolean): Promise<SetupRequest> {
    const target = append ? draft.publication?.library_id : undefined;
    if (append && !target) throw new SetupUnavailable("Choose a previously published Library entry before adding a revision.", 409);
    const path = target ? `${this.base}/fleet-libraries/${encodeURIComponent(target)}/revisions` : `${this.base}/fleet-libraries`;
    const body = target ? {draft_candidate_id: draft.candidate.id} : {draft_candidate_id: draft.candidate.id, label};
    const current = target ? await this.request({path: `${this.base}/fleet-libraries/${encodeURIComponent(target)}`}, schema.object({etag: schema.string()})) : null;
    return {path, method: REQUEST_METHOD.post, body, headers: current ? {[SETUP_HEADERS.match]: current.etag} : {}};
  }

  async publish(request: SetupRequest, key: string): Promise<string> {
    const receipt = await this.request({...request, headers: {...request.headers, [SETUP_HEADERS.key]: key}}, PublishedSchema);
    return `Published Library revision ${receipt.revision_id}. Running fleets keep their pinned setup.`;
  }

  async stage(draft: ReadyDraft, label: string, key: string): Promise<string> {
    if (!this.platform) throw new SetupUnavailable("Platform Library operator scope is required.", 403);
    const body = {workspace_id: this.workspace, draft_candidate_id: draft.candidate.id, label};
    const receipt = PublishedSchema.parse(await this.platform(body, key));
    return `Platform entry ${receipt.library_id} is staged. Publish it explicitly in the platform Library.`;
  }
}
