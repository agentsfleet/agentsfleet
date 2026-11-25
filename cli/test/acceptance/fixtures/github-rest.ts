// GitHub's REST API, as the test account whose fine-grained token the journey
// is given: a real pull request on a real repository, opened and closed the
// way a person does it.
//
// The token is the caller's and is sent only in the request's own header. No
// failure here carries it: GitHub names what went wrong with a status and a
// message.

import { Option, Schema } from "effect";

export type GithubMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

/** How long one call may take before it is abandoned. */
export const GITHUB_REQUEST_TIMEOUT_MS = 30_000;

const API = "https://api.github.com";
// https://docs.github.com/rest/about-the-rest-api/api-versions
const API_VERSION = "2022-11-28";
// A generic client name: GitHub refuses a request with no User-Agent.
const USER_AGENT = "acceptance-suite";
const HTTP_NO_CONTENT = 204;

const RefusalSchema = Schema.Struct({ message: Schema.String });

/**
 * One call as the token's account: the parsed body, or `null` for an answer
 * with none. A refusal names GitHub's status and message.
 */
export async function callGithub(
  token: string, method: GithubMethod, path: string, body?: Readonly<Record<string, unknown>>,
): Promise<unknown> {
  const response = await fetch(`${API}${path}`, {
    method,
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "User-Agent": USER_AGENT,
      "X-GitHub-Api-Version": API_VERSION,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    redirect: "error",
    signal: AbortSignal.timeout(GITHUB_REQUEST_TIMEOUT_MS),
  });
  if (response.status === HTTP_NO_CONTENT) return null;
  const answer: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const refused = Schema.decodeUnknownOption(RefusalSchema)(answer);
    const message = Option.isSome(refused) ? refused.value.message : "no message";
    throw new Error(`GitHub answered ${response.status} to ${method} ${path}: ${message}`);
  }
  return answer;
}
