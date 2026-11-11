import type { MessageState } from "@assistant-ui/react";
import { ACTOR, baseActorOf, eventLinkFrom, eventReferenceFrom, parsePayload, senderLabelFor, SENDER, steerMessageFrom } from "@/lib/events/event-summary";
import { FLEET_ACTIVITY } from "@/lib/events/fleet-activity";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";
import { eventOutcome, messageOutcome } from "./fleetFailureCopy";
import { hasOwnContent, readActor, readCustomStatus, readGroupMembers, readRenderKind, readRequestJson, readText, type RecordedActivity } from "./fleetMessageReaders";
import { STATUS_AGENT_ERROR, STATUS_IN_FLIGHT, STATUS_OPTIMISTIC } from "./fleetMessageStatus";
import { GROUP_META, RENDER_KIND } from "./useFleetThreadEntries";
import { CHAT_ELEMENT, type ActivityDetails, type ActivityElement, type FleetChatElement } from "./fleet-chat-elements";

/** One ordered event becomes presentation elements; this never performs work. */
export function fleetChatElements(message: MessageState): readonly FleetChatElement[] {
  if (message.metadata.custom[GROUP_META.MEMBERS] !== undefined) {
    const records = readGroupMembers(message);
    if (!records) return [defaultElement(message, "Some activity details couldn’t be loaded.")];
    const [first, ...rest] = records.map(recordedActivityElement);
    return first ? [{kind: CHAT_ELEMENT.GROUP, data: {members: [first, ...rest]}}] : [];
  }
  const status = readCustomStatus(message);
  switch (message.role) {
    case "system": return [activityElement({
      eventId: message.id, actor: readActor(message), status, createdAt: message.createdAt,
      text: readText(message), payload: readRequestJson(message), outcome: messageOutcome(message),
      split: readRenderKind(message) === RENDER_KIND.TRIGGER,
    })];
    case "assistant": return [{kind: CHAT_ELEMENT.REPLY, data: {message, status}}];
    case "user": return operatorElements(message, status);
    default: return [defaultElement(message, null)];
  }
}

function defaultElement(message: MessageState, notice: string | null): FleetChatElement<"default-activity"> {
  return {kind: CHAT_ELEMENT.DEFAULT, data: {
    eventId: message.id, actor: readActor(message), status: readCustomStatus(message), createdAt: message.createdAt,
    headline: readText(message).trim() || "Activity update", outcome: messageOutcome(message) || undefined,
    failed: readCustomStatus(message) === STATUS_AGENT_ERROR, details: {kind: "none"}, source: null, notice,
  }};
}

function operatorElements(message: MessageState, status: string): FleetChatElement[] {
  const elements: FleetChatElement[] = [];
  if (hasOwnContent(message)) elements.push({kind: CHAT_ELEMENT.OPERATOR, data: {
    actor: readActor(message), text: readText(message), createdAt: message.createdAt,
    optimistic: status === STATUS_OPTIMISTIC,
  }});
  if (readRenderKind(message) !== RENDER_KIND.TRIGGER) elements.push({kind: CHAT_ELEMENT.REPLY, data: {message, status}});
  return elements;
}

function recordedActivityElement(record: RecordedActivity): ActivityElement {
  const element = activityElement({eventId: record.id, actor: record.actor, status: record.status,
    createdAt: record.createdAt, text: record.text, payload: record.custom?.requestJson,
    outcome: eventOutcome(record), split: false});
  // The collapsed row must retain its source reference while its members are hidden.
  return {...element, data: {...element.data, headline: record.text, source: null, outcome: eventOutcome(record)}};
}

type ActivityInput = {
  eventId: string; actor: string; status: string; createdAt: Date; text: string;
  payload: string | null | undefined; outcome: string; split: boolean;
};

function activityElement(input: ActivityInput): ActivityElement {
  const recorded = input.status === FLEET_ACTIVITY.status;
  const schedule = senderLabelFor(input.actor) === SENDER.SCHEDULE;
  const approval = baseActorOf(input.actor) === ACTOR.GATE_BLOCKED || input.status === AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED;
  const link = recorded || schedule ? null : eventLinkFrom(input.payload);
  const reference = link ? eventReferenceFrom(input.payload) : null;
  const data = {
    eventId: input.eventId, actor: input.actor, status: input.status, createdAt: input.createdAt,
    headline: activityHeadline(input.text, reference),
    outcome: input.status === STATUS_IN_FLIGHT || input.split || recorded ? undefined : input.outcome,
    failed: input.status === STATUS_AGENT_ERROR,
    details: activityDetails(input.actor, recorded || schedule ? undefined : input.payload),
    source: link ? {href: link, label: reference ?? "View source", repositoryReference: reference !== null} : null,
  };
  if (schedule) return {kind: CHAT_ELEMENT.WAKEUP, data};
  if (approval) return {kind: CHAT_ELEMENT.APPROVAL, data};
  return {kind: CHAT_ELEMENT.INTEGRATION, data};
}

function activityDetails(actor: string, payload: string | null | undefined): ActivityDetails {
  const message = actor.startsWith("slack:") ? steerMessageFrom(payload) : "";
  if (message) return {kind: "message", text: message};
  const record = parsePayload(payload);
  return payload && (!record || Object.keys(record).length > 0) ? {kind: "payload", json: payload} : {kind: "none"};
}

function activityHeadline(headline: string, reference: string | null): string {
  if (!reference) return headline;
  const index = headline.indexOf(reference);
  if (index < 0) return headline;
  const prefix = headline.slice(0, index).trimEnd();
  const suffix = headline.slice(index + reference.length).trimStart();
  if (suffix.startsWith("—")) return prefix ? `${prefix.endsWith("·") ? prefix : `${prefix} ·`} ${suffix.slice(1).trimStart()}` : suffix.slice(1).trimStart();
  if (!suffix && prefix.endsWith("·")) return prefix.slice(0, -1).trimEnd();
  return `${prefix}${suffix ? ` ${suffix}` : ""}`.trim();
}
