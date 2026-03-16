/**
 * CLI banner for --help and --version output.
 */

export function printBanner(stream, version, opts = {}) {
  const noColor = opts.noColor || false;
  const jsonMode = opts.jsonMode || false;

  if (jsonMode) return;

  if (noColor) {
    stream.write(`zombiectl v${version}\n`);
    return;
  }

  stream.write(` \u{1F7E7}\n`);
  stream.write(`\u{1F7E7}\u{1F7E9} zombiectl v${version}\n`);
  stream.write(`\u{1F7E9} \u{1F7E7}\n`);
}
