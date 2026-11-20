import type { TurnDetail } from "@/lib/api/turns";
import { ACTOR, TURN_STATUS } from "@/lib/turns/turn-summary";

export const ACCOUNT_ID = "user_3gkbgxjnujsxbdxttcwcslpc87k";
export const PLATFORM_IDENTITY = "github-app";

export function row(over: Partial<TurnDetail> = {}): TurnDetail {
  return {
    turn_id: "e1",
    fleet_id: "f1",
    workspace_id: "ws1",
    actor: ACTOR.FLEET,
    event_type: "chat",
    status: TURN_STATUS.PROCESSED,
    request_json: "{}",
    response_text: null,
    tool_calls: null,
    tokens: null,
    wall_ms: null,
    failure_label: null,
    failure_detail: null,
    resumes_turn_id: null,
    cost_nanos: null,
    created_at: "1970-01-01T00:00:00.001Z",
    updated_at: "1970-01-01T00:00:00.001Z",
    ...over,
  };
}
