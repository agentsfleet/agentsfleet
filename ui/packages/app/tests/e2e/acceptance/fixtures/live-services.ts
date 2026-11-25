/**
 * live-services.ts — the two services a deployed stack is woken by, as a
 * journey reaches them: GitHub's REST API, called as the test account whose
 * token the journey is given, and Slack's Web API, called as the test person.
 *
 * Nothing here stands in for either service. The daemon drops what the
 * platform caused itself, so a journey acts on GitHub and in Slack as a person
 * would; the events that wake the fleet are the ones GitHub and Slack send.
 * A token is sent only in its request's own header, and no failure here
 * carries one.
 */
import { z } from "zod";

export type GithubMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

const GITHUB_API = "https://api.github.com";
// https://docs.github.com/rest/about-the-rest-api/api-versions
const GITHUB_API_VERSION = "2022-11-28";
const SLACK_API = "https://slack.com/api";
// A generic client name: GitHub refuses a request with no User-Agent.
const USER_AGENT = "acceptance-journey";
const REQUEST_TIMEOUT_MS = 30_000;
const HTTP_NO_CONTENT = 204;
const GithubRefusal = z.object({message: z.string()});
const SlackAnswer = z.object({ok: z.boolean(), error: z.string().optional()});

/**
 * The variables a journey cannot run without. Any that is unset fails the
 * journey in one sentence naming each; a value is never printed.
 */
export function requiredVariables<Name extends string>(names: readonly Name[]): Record<Name, string> {
  const missing = names.filter((name) => !process.env[name]);
  if (missing.length > 0) {
    throw new Error(`${missing.join(", ")} ${missing.length === 1 ? "is" : "are"} not set; this journey runs against a deployed stack and needs ${missing.length === 1 ? "it" : "each"}`);
  }
  return Object.fromEntries(names.map((name) => [name, process.env[name] ?? ""])) as Record<Name, string>;
}

/** One variable a journey cannot run without, as `requiredVariables` reads it. */
export function requiredVariable<Name extends string>(name: Name): string {
  return requiredVariables([name])[name];
}

/**
 * One GitHub REST call as the account whose token this is: the parsed body,
 * or `null` for an answer with none. A refusal names GitHub's status and
 * message.
 */
export async function callGithub<T>(
  token: string, method: GithubMethod, path: string, schema: z.ZodType<T>, body?: Readonly<Record<string, unknown>>,
): Promise<T> {
  const response = await fetch(`${GITHUB_API}${path}`, {
    method, body: body === undefined ? undefined : JSON.stringify(body), redirect: "error", signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    headers: {Accept: "application/vnd.github+json", Authorization: `Bearer ${token}`, "User-Agent": USER_AGENT, "X-GitHub-Api-Version": GITHUB_API_VERSION},
  });
  if (response.status === HTTP_NO_CONTENT) return schema.parse(null);
  const answer: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const refused = GithubRefusal.safeParse(answer);
    throw new Error(`GitHub answered ${response.status} to ${method} ${path}: ${refused.success ? refused.data.message : "no message"}`);
  }
  return schema.parse(answer);
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
