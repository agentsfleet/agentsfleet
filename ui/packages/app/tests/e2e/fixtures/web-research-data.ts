import curiousOtter from "../../../../../../tests/fixtures/fleets/curious-otter.json";
import type {TurnDetail} from "@/lib/api/turns";
import {FLEET_ID, WORKSPACE} from "@/tests/helpers/fleet-fixtures";

const [question] = curiousOtter.follow_ups;
if (!question) throw new Error("curious-otter has no research question");

/** The saved turn the customer can reopen in the chat. Only its API data is simulated. */
export const RESEARCH = {
  name: curiousOtter.name, question,
  query: "IANA example domains documentation",
  sources: [
    {title: "Example Domains", url: "https://www.iana.org/help/example-domains"},
    {title: "IANA-managed Reserved Domains", url: "https://www.iana.org/domains/reserved"},
  ],
  answer: "IANA reserves example.com for documentation, so you can use it in a guide without registering it.",
} as const;

export function researchTurn(base: TurnDetail, plain: boolean): TurnDetail {
  const [first] = RESEARCH.sources;
  const citation = plain ? first.url : `[${first.title}](${first.url})`;
  return {...base, workspace_id: WORKSPACE, fleet_id: FLEET_ID, actor: "steer:fixture-owner", event_type: "chat",
    status: "processed", request_json: JSON.stringify({message: RESEARCH.question}),
    wall_ms: 7800, response_text: `${RESEARCH.answer}\n\n${citation}`, tool_calls: {calls: [
      {call_id: "1:1", name: "web_search", arguments: {query: RESEARCH.query}, status: "succeeded", duration_ms: 300,
        sources: [...RESEARCH.sources]},
      {call_id: "1:2", name: "web_fetch", arguments: {url: first.url}, status: "succeeded", duration_ms: 620,
        sources: [{url: first.url}]},
    ]}};
}
