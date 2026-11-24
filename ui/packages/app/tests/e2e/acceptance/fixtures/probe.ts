/**
 * probe.ts — what a probe fleet is asked, and the one line it answers.
 *
 * A fleet holds no instructions, so a probe's whole request rides in its
 * message. The request never spells the expected reply literally, so finding
 * that reply on the page cannot match the person's own message.
 */

export const EXECUTION_REPLY_PREFIX = "ACK";
export const ACCESS_REPLY_PREFIX = "GRANTED";

/** A message the fleet answers with one known line and no tool call. */
export function probeMessage(token: string): string {
  return `Reply with exactly one line: the word ${EXECUTION_REPLY_PREFIX}, a space, then the token ${token}. ` +
    "Do not call any tool. Do not add anything before or after that line.";
}

/** The line a probe answers `probeMessage(token)` with. */
export function probeReply(token: string): string {
  return `${EXECUTION_REPLY_PREFIX} ${token}`;
}

/** Asks the fleet to request a workspace credential, then to say it holds it once a person approves. */
export function accessMessage(name: string, token: string): string {
  return `Call request_access with the name ${name}. Once a person approves it, reply with exactly one line: ` +
    `the word ${ACCESS_REPLY_PREFIX}, a space, then the token ${token}. Do not add anything before or after that line.`;
}

/** The line a fleet answers `accessMessage(name, token)` with once it holds the credential. */
export function accessReply(token: string): string {
  return `${ACCESS_REPLY_PREFIX} ${token}`;
}
