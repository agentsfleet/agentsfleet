// The fleet address: `@name` at the start of a text names the fleet it is for.
//
// The dashboard composer carries the same rule (ui/packages/app/lib/chat-address.ts),
// and both suites read every case in tests/fixtures/fleet-address/cases.json,
// which is what keeps the two parsers from drifting. Slack does not use it.

/** A text that opens with an address: the fleet's name, and the words for it. */
export interface FleetAddress {
  /** The saved name the address spells, lower-cased. */
  readonly name: string;
  /** The message after the address; empty when the text is the address alone. */
  readonly rest: string;
}

// After any leading whitespace: `@`, a name of 1 to 64 ASCII letters, digits or
// `-`, then the end of the text, whitespace, `:` or `,`. That closing character
// and the whitespace after it belong to the address, not the message. The name's
// characters and the closing ones share none, so a 65th name character fails
// the match rather than being cut off into a shorter name.
const ADDRESS = /^\s*@([A-Za-z0-9-]{1,64})(?:$|[\s:,]\s*)/u;

/** The address `text` opens with, or undefined when the text is ordinary words. */
export function readFleetAddress(text: string): FleetAddress | undefined {
  const [address, name] = ADDRESS.exec(text) ?? [];
  if (address === undefined || name === undefined) return undefined;
  return { name: name.toLowerCase(), rest: text.slice(address.length) };
}
