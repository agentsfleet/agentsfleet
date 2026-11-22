import { BrandMark, Button, cn } from "@agentsfleet/design-system";
import { STORY_DETAIL, STORY_ROLE, type StoryEntry } from "../lib/chat-story";
import type { ExampleJob } from "../lib/example-jobs";

export function StoryMessage({
  entry,
  job,
  onEvidence,
}: {
  entry: StoryEntry;
  job: ExampleJob;
  onEvidence: (index: number, citation: HTMLButtonElement) => void;
}) {
  const { step, text, complete } = entry;
  if (step.role === STORY_ROLE.event)
    return (
      <p className="my-xl flex items-start gap-md border-y border-border py-lg text-label text-text-muted">
        <span aria-hidden="true" className="text-pulse">
          ◇
        </span>
        {text}
      </p>
    );
  const user = step.role === STORY_ROLE.user;
  return (
    <div
      className={cn(
        "my-xl",
        user &&
          "ml-auto w-fit max-w-[94%] rounded-lg rounded-br-sm bg-muted p-lg md:max-w-[88%]",
      )}
    >
      {step.role === STORY_ROLE.fleet && (
        <div className="mb-md flex items-center gap-md text-label text-text-muted">
          <BrandMark className="size-[1.375rem] shrink-0 text-pulse" />
          <span>{job.name}</span>
        </div>
      )}
      <div className={cn(!user && "md:ml-[1.875rem]")}>
        <p className="whitespace-pre-wrap text-body-sm leading-body text-text">
          {text}
          {!complete && (
            <span
              className="ml-xs inline-block h-[1em] w-0.5 bg-pulse align-text-bottom group-data-[playing=true]/chat:motion-safe:animate-pulse"
              aria-hidden="true"
            />
          )}
        </p>
        {complete && step.detail === STORY_DETAIL.evidence && (
          <div className="mt-lg flex flex-wrap gap-sm">
            {job.evidence.map((item, index) => (
              <Button
                key={item.label}
                variant="outline"
                size="sm"
                className="min-h-11 text-xs"
                onClick={(event) => onEvidence(index, event.currentTarget)}
              >
                E{index + 1} · {item.source}
              </Button>
            ))}
          </div>
        )}
        {complete && step.detail === STORY_DETAIL.proposal && (
          <pre
            className="mt-lg rounded-md border border-border bg-surface-deep p-lg font-mono text-xs leading-body whitespace-pre-wrap"
            aria-label="Proposed change, not applied"
          >
            {job.code.map((line) => (
              <span
                key={line}
                className={
                  line.startsWith("+")
                    ? "text-success"
                    : line.startsWith("-")
                      ? "text-error"
                      : ""
                }
              >
                {line}
                {"\n"}
              </span>
            ))}
          </pre>
        )}
      </div>
    </div>
  );
}
