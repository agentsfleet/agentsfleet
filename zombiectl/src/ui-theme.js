// NO_COLOR spec: any non-empty value disables color
const useColor = !process.env.NO_COLOR && process.stdout.isTTY === true;

function color(code, text) {
  return useColor ? `\u001b[${code}m${text}\u001b[0m` : text;
}

export const ui = {
  ok:    (s) => color("32",   `✔ ${s}`),
  info:  (s) => color("36",   `ℹ ${s}`),
  warn:  (s) => color("33",   `▲ ${s}`),
  err:   (s) => color("31",   `✖ ${s}`),
  head:  (s) => color("1;36", s),
  dim:   (s) => color("2",    s),
  label: (s) => color("2",    s),          // dim alias for KV labels
  run:   (s) => color("1;35", `◉ ${s}`),  // bold magenta — active/running state
  step:  (s) => color("1;37", s),          // bold white — numbered steps
};

export function printSection(stream, title, theme = ui) {
  const heading = theme.head ? theme.head(title) : title;
  const rule = "─".repeat(title.length);
  stream.write(`\n${heading}\n`);
  stream.write(`${theme.dim ? theme.dim(rule) : rule}\n`);
}

export function printKeyValue(stream, rows, theme = ui) {
  const entries = Object.entries(rows);
  if (entries.length === 0) return;
  const width = Math.max(...entries.map(([k]) => k.length), 0);
  const sep = theme.dim ? theme.dim("  ·  ") : "  ·  ";
  for (const [key, value] of entries) {
    const label = theme.dim ? theme.dim(key.padEnd(width)) : key.padEnd(width);
    stream.write(`  ${label}${sep}${value}\n`);
  }
}

export function printTable(stream, columns, rows, theme = ui) {
  if (rows.length === 0) {
    const none = theme.dim ? theme.dim("(none)") : "(none)";
    stream.write(`${none}\n`);
    return;
  }
  const widths = columns.map((c) =>
    Math.max(
      c.label.length,
      ...rows.map((r) => String(r[c.key] ?? "").length),
    ),
  );

  const headerStr = columns.map((c, i) => c.label.padEnd(widths[i])).join("  ");
  stream.write(`${theme.head ? theme.head(headerStr) : headerStr}\n`);

  const sepStr = widths.map((w) => "\u2500".repeat(w)).join("  ");
  stream.write(`${theme.dim ? theme.dim(sepStr) : sepStr}\n`);

  for (const row of rows) {
    stream.write(
      `${columns.map((c, i) => String(row[c.key] ?? "").padEnd(widths[i])).join("  ")}\n`,
    );
  }
}
