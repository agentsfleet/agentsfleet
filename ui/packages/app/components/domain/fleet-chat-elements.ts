import type { MessageState } from "@assistant-ui/react";

export const CHAT_ELEMENT = {
  OPERATOR: "operator-turn",
  REPLY: "reply-turn",
  INTEGRATION: "integration-event",
  WAKEUP: "scheduled-wakeup",
  APPROVAL: "approval-event",
  GROUP: "activity-group",
  DEFAULT: "default-activity",
} as const;

export type ActivityDetails =
  | {kind: "none"}
  | {kind: "message"; text: string}
  | {kind: "payload"; json: string};

export type ActivityElementData = {
  eventId: string;
  actor: string;
  status: string;
  createdAt: Date;
  headline: string;
  outcome: string | undefined;
  failed: boolean;
  details: ActivityDetails;
  source: {href: string; label: string; repositoryReference: boolean} | null;
};

/** Payloads are selected by adapters; rows never interpret event metadata. */
export type FleetChatElementData = {
  "operator-turn": {actor: string; text: string; createdAt: Date; optimistic: boolean};
  "reply-turn": {message: MessageState; status: string};
  "integration-event": ActivityElementData;
  "scheduled-wakeup": ActivityElementData;
  "approval-event": ActivityElementData;
  "activity-group": {members: readonly [ActivityElement, ...ActivityElement[]]};
  "default-activity": ActivityElementData & {notice: string | null};
};

export type ChatElementKind = keyof FleetChatElementData;
export type FleetChatElement<K extends ChatElementKind = ChatElementKind> = {
  [P in K]: {kind: P; data: FleetChatElementData[P]}
}[K];
export type ActivityElement = FleetChatElement<"integration-event" | "scheduled-wakeup" | "approval-event">;
