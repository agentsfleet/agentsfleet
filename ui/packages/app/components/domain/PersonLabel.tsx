import { fallbackPersonLabel, systemLabel } from "@/lib/identity/person";

/**
 * A person, by the name they go by.
 *
 * The name arrives on the same row that carried the subject: `resolved_by` is
 * the identifier of record, `resolved_by_name` is what this deployment's own
 * `core.users` row calls them. So this component renders and never fetches.
 * There is no loading state because there is no lookup, and no placeholder to
 * flash, because the answer arrived with the row.
 *
 * An empty name is ordinary rather than a failure: a pending gate has no
 * decider, the daemon's sentinels are not people, and a subject that never
 * signed up on this deployment has no row. All three fall to the shortened
 * subject, with the full string one hover away — a record of who decided is
 * better printed ugly than dropped.
 */
export function PersonLabel({
  actor,
  name,
  className,
}: {
  actor: string;
  name: string;
  className?: string;
}) {
  if (actor.length === 0) return null;
  return (
    <span className={className} title={actor}>
      {systemLabel(actor) ?? (name || fallbackPersonLabel(actor))}
    </span>
  );
}
