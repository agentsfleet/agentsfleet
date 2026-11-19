// The address rule a chat message is read by: `@name` at its start sends it to
// the fleet saved under that name. The command line reads the same rule
// (cli/src/lib/fleet-address.ts), and both suites assert every case in
// tests/fixtures/fleet-address/cases.json, so the two parsers cannot drift.
// Slack does not use it: there the bot is the mention and the first word names
// the fleet.

/** A message addressed to a fleet: the fleet's saved name, lower-cased, and the words for it. */
export type ChatAddress = { name: string; rest: string };

// After leading whitespace, `@` and the run of ASCII letters, digits and `-`,
// then the end, or one whitespace, `:` or `,` with the whitespace after it.
// The run is greedy and every shorter prefix of it is followed by another name
// character, so a run ended by any other character never matches as a shorter
// name.
const ADDRESS = /^\s*@([A-Za-z0-9-]+)(?:$|[\s:,]\s*)/;
// A fleet name's bound (`FleetName` in the daemon). The name is ASCII, so its
// bytes are its characters.
const NAME_MAX_BYTES = 64;

/** The address a message opens with, or null when it is ordinary words. */
export function parseChatAddress(text: string): ChatAddress | null {
  const match = ADDRESS.exec(text);
  const name = match?.[1];
  if (match === null || name === undefined || name.length > NAME_MAX_BYTES) return null;
  return { name: name.toLowerCase(), rest: text.slice(match[0].length) };
}
