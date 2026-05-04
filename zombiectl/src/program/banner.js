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

  stream.write(`    ${c("1;38;5;208", `\u256D${bar}\u256E`)}\n`);
  stream.write(` \u{1F9DF} ${c("1;38;5;208", "\u2502")}${c("1;37", label)}${c("1;38;5;208", "\u2502")}\n`);
  stream.write(`    ${c("1;38;5;208", `\u2570${bar}\u256F`)}\n`);
  stream.write(`    ${c("2", "  autonomous agent cli")}\n`);
}

export function printPreReleaseWarning(stream, opts = {}) {
  const noColor = opts.noColor || false;
  const jsonMode = opts.jsonMode || false;
  const ttyOnly = opts.ttyOnly || false;

  if (jsonMode) return;
  if (ttyOnly) return;

  if (noColor) {
    stream.write(`\n[PRE-RELEASE] This is a pre-release build for early access testing.\n`);
    stream.write(`Contact nkishore@megam.io to get access.\n\n`);
    return;
  }

  const c = (code, s) => `\u001b[${code}m${s}\u001b[0m`;
  stream.write(`\n  ${c("1;33", "⚠  Pre-release build")} — not for production use.\n`);
  stream.write(`     Early access testing only. Contact ${c("1;37", "nkishore@megam.io")} to get access.\n\n`);
}
