import type { EventDetail } from "@/lib/api/events";
import { ACTOR, EVENT_STATUS } from "@/lib/events/event-summary";

export const ACCOUNT_ID = "user_3gkbgxjnujsxbdxttcwcslpc87k";
export const PLATFORM_IDENTITY = "github-app";

export function row(over: Partial<EventDetail> = {}): EventDetail {
  return {
    event_id: "e1",
    fleet_id: "f1",
    workspace_id: "ws1",
    actor: ACTOR.FLEET,
    event_type: "chat",
    status: EVENT_STATUS.PROCESSED,
    request_json: "{}",
    response_text: null,
    tokens: null,
    wall_ms: null,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: null,
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}
