import type { DurableTaskStatement } from "@/lib/api/tasks";

/** One completion statement, read as the decision model judges it: a question and what yes and no mean. */
export function TaskStatement({number, statement}: {number: number; statement: DurableTaskStatement}) {
  const {instructions, criteria} = statement.question;
  return <div className="space-y-xs text-label leading-label">
    <p className="break-words font-medium">{number}. {instructions}</p>
    <dl className="space-y-xs text-muted-foreground">
      <div className="break-words"><dt className="inline font-medium">Yes: </dt><dd className="inline">{criteria.true}</dd></div>
      <div className="break-words"><dt className="inline font-medium">No: </dt><dd className="inline">{criteria.false}</dd></div>
    </dl>
  </div>;
}
