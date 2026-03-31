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

  const c = (code, s) => `\u001b[${code}m${s}\u001b[0m`;
  const bar = "\u2500".repeat(label.length);

  stream.write(`    ${c("1;36", `\u256D${bar}\u256E`)}\n`);
  stream.write(` \u{1F9DF} ${c("1;36", "\u2502")}${c("1;37", label)}${c("1;36", "\u2502")}\n`);
  stream.write(`    ${c("1;36", `\u2570${bar}\u256F`)}\n`);
  stream.write(`    ${c("2", "  autonomous agent cli")}\n`);
}
