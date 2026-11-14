import type { z } from "zod";
import type { Draft } from "./draft-types";
import type { SetupRequest } from "./draft-request";
import { z as schema } from "zod";
import { PublishedSchema, SETUP_HEADERS } from "./draft-types";
import { REQUEST_METHOD, SetupUnavailable } from "./draft-request";

type Request = <T>(request: SetupRequest, parser: z.ZodType<T>) => Promise<T>;
type ReadyDraft = Draft & {candidate: NonNullable<Draft["candidate"]>};
export const PUBLISHED_NOTICE = "Setup published.";

/** Plugin writes retain the operation identity supplied by the private chat owner. */
export class DraftPublication {
  constructor(private readonly base: string,
    private readonly request: Request) {}


  async prepare(draft: ReadyDraft, label: string, append: boolean, selectedTarget?: string): Promise<SetupRequest> {
    const target = append ? selectedTarget ?? draft.publication?.plugin_id : undefined;
    if (append && !target) throw new SetupUnavailable("Choose a previously published Plugin entry before adding a revision.", 409);
    const path = target ? `${this.base}/plugins/${encodeURIComponent(target)}/revisions` : `${this.base}/plugins`;
    const body = target ? {draft_candidate_id: draft.candidate.id} : {draft_candidate_id: draft.candidate.id, label};
    const current = target ? await this.request({path: `${this.base}/plugins/${encodeURIComponent(target)}`}, schema.object({etag: schema.string()})) : null;
    return {path, method: REQUEST_METHOD.post, body, headers: current ? {[SETUP_HEADERS.match]: current.etag} : {}};
  }

  async publish(request: SetupRequest, key: string): Promise<string> {
    await this.request({...request, headers: {...request.headers, [SETUP_HEADERS.key]: key}}, PublishedSchema);
    return PUBLISHED_NOTICE;
  }

}
