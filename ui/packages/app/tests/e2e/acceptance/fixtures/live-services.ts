/**
 * live-services.ts — the two services a deployed stack is woken by, as a
 * journey reaches them: GitHub's public REST, read with no credential, and
 * Slack's Web API, called as the test person.
 *
 * Nothing here stands in for either service. A journey reads what the fleet
 * did on GitHub, and says in Slack what a person would say; the events that
 * wake the fleet are the ones GitHub and Slack send.
 */
import { z } from "zod";

const GITHUB_API = "https://api.github.com";
const SLACK_API = "https://slack.com/api";
// A generic client name: GitHub refuses a request with no User-Agent.
const USER_AGENT = "acceptance-journey";
const REQUEST_TIMEOUT_MS = 30_000;
const HTTP_NOT_FOUND = 404;
const SlackAnswer = z.object({ok: z.boolean(), error: z.string().optional()});

/** A variable the journey cannot run without, or a sentence naming it. The value is never printed. */
export function requiredVariable(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set; this journey runs against a deployed stack and needs it`);
  return value;
}

/** One read of GitHub's public REST: the parsed body, or `null` for a thing that is not there. */
export async function readGithub<T>(path: string, schema: z.ZodType<T>): Promise<T | null> {
  const response = await fetch(`${GITHUB_API}${path}`, {
    headers: {Accept: "application/vnd.github+json", "User-Agent": USER_AGENT}, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (response.status === HTTP_NOT_FOUND) {
    await response.body?.cancel();
    return null;
  }
  if (!response.ok) throw new Error(`GitHub answered ${response.status} for ${path}`);
  return schema.parse(await response.json());
}

/** One Slack Web API call as the person whose token this is. A refusal names Slack's reason and nothing else. */
export async function callSlack<T>(token: string, method: string, params: Readonly<Record<string, string>>, schema: z.ZodType<T>): Promise<T> {
  const response = await fetch(`${SLACK_API}/${method}`, {
    method: "POST", body: new URLSearchParams(params), signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    headers: {Authorization: `Bearer ${token}`},
  });
  if (!response.ok) throw new Error(`Slack answered ${response.status} for ${method}`);
  const body: unknown = await response.json();
  const answer = SlackAnswer.parse(body);
  if (!answer.ok) throw new Error(`Slack refused ${method}: ${answer.error ?? "no reason given"}`);
  return schema.parse(body);
}
