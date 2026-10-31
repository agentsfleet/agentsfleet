export type NumberedRow = { number: number | null; text: string };

/** A null number marks a gap in the retained output. Text stays literal. */
export function FleetOutputLine({ row }: { row: NumberedRow }) {
  return <>
    <span data-line-number={row.number ?? undefined} aria-hidden="true" className="select-none text-right tabular-nums text-text-subtle">
      {row.number ?? "⋯"}
    </span>
    <span className="min-w-0 whitespace-pre-wrap break-words">{row.text}</span>
  </>;
}
