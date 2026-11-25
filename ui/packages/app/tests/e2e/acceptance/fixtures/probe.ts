/**
 * probe.ts — what a probe fleet is asked, and the one line it answers.
 *
 * A fleet holds no instructions, so a probe's whole request rides in its
 * message. The request never spells the expected reply literally, so finding
 * that reply on the page cannot match the person's own message.
 */

export const EXECUTION_REPLY_PREFIX = "ACK";

/** A message the fleet answers with one known line and no tool call. */
export function probeMessage(token: string): string {
  return `Reply with exactly one line: the word ${EXECUTION_REPLY_PREFIX}, a space, then the token ${token}. ` +
    "Do not call any tool. Do not add anything before or after that line.";
}

/** The line a probe answers `probeMessage(token)` with, and the line a reply to `tokenMessage` ends with. */
export function probeReply(token: string): string {
  return `${EXECUTION_REPLY_PREFIX} ${token}`;
}

/**
 * A fixture fleet's own message with this run's token. The fleet keeps the job
 * the message gives it, and its reply ends with the one line a journey can find.
 */
export function tokenMessage(message: string, token: string): string {
  return `${message} End your reply with the word ${EXECUTION_REPLY_PREFIX}, a space, then the token ${token}.`;
}
