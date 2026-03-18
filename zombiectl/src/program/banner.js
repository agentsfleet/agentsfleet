/**
 * CLI banner for --help and --version output.
 */

export function printBanner(stream, version, opts = {}) {
  const noColor = opts.noColor || false;
  const jsonMode = opts.jsonMode || false;

  if (jsonMode) return;

  const label = `  zombiectl v${version}   `;

  if (noColor) {
    stream.write(`zombiectl v${version}\n`);
    return;
  }

  const bar = "\u2500".repeat(label.length);
  stream.write(`    \u256D${bar}\u256E\n`);
  stream.write(` \u{1F9DF} \u2502${label}\u2502\n`);
  stream.write(`    \u2570${bar}\u256F\n`);
}
