const useColor =
  process.env.NO_COLOR !== "1" &&
  process.env.NO_COLOR !== "true" &&
  process.stdout.isTTY === true;

function color(code, text) {
  return useColor ? `\u001b[${code}m${text}\u001b[0m` : text;
}

export const ui = {
  ok: (s) => color("32", `✔ ${s}`),
  info: (s) => color("36", `i ${s}`),
  warn: (s) => color("33", `! ${s}`),
  err: (s) => color("31", `✖ ${s}`),
  head: (s) => color("1;36", s),
  dim: (s) => color("2", s),
};

export function printSection(stream, title, theme = ui) {
  stream.write(`${theme.head ? theme.head(title) : title}\n`);
}

export function printKeyValue(stream, rows) {
  const entries = Object.entries(rows);
  const width = Math.max(...entries.map(([k]) => k.length), 0);
  for (const [key, value] of entries) {
    stream.write(`${key.padEnd(width)} : ${value}\n`);
  }
}

export function printTable(stream, columns, rows) {
  if (rows.length === 0) {
    stream.write("(none)\n");
    return;
  }
  const widths = columns.map((c) =>
    Math.max(
      c.label.length,
      ...rows.map((r) => String(r[c.key] ?? "").length),
    ),
  );

  const header = columns
    .map((c, i) => c.label.padEnd(widths[i]))
    .join("  ");
  stream.write(`${header}\n`);
  stream.write(`${widths.map((w) => "-".repeat(w)).join("  ")}\n`);

  for (const row of rows) {
    stream.write(
      `${columns.map((c, i) => String(row[c.key] ?? "").padEnd(widths[i])).join("  ")}\n`,
    );
  }
}
