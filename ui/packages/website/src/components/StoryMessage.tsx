import { BrandMark, Button } from "@agentsfleet/design-system";
import { STORY_DETAIL, STORY_ROLE, type StoryEntry } from "../lib/chat-story";
import type { ExampleJob } from "../lib/example-jobs";

export function StoryMessage({ entry, job, onEvidence }: { entry: StoryEntry; job: ExampleJob; onEvidence: (index: number, citation: HTMLButtonElement) => void }) {
  const { step, text, complete } = entry;
  if (step.role === STORY_ROLE.event) return <p className="chat-milestone"><span aria-hidden="true">◇</span>{text}</p>;
  return (
    <div className={step.role === STORY_ROLE.user ? "fleet-assignment" : "fleet-message"}>
      {step.role === STORY_ROLE.fleet && <div className="fleet-message-header"><BrandMark className="chat-avatar" /><span>{job.name}</span></div>}
      <div className="fleet-message-body">
        <p>{text}{!complete && <span className="chat-caret" aria-hidden="true" />}</p>
        {complete && step.detail === STORY_DETAIL.evidence && <div className="chat-citations">
          {job.evidence.map((item, index) => <Button key={item.label} variant="outline" size="sm" onClick={(event) => onEvidence(index, event.currentTarget)}>E{index + 1} · {item.source}</Button>)}
        </div>}
        {complete && step.detail === STORY_DETAIL.proposal && <pre className="chat-diff" aria-label="Proposed change, not applied">
          {job.code.map((line) => <span key={line} className={line.startsWith("+") ? "diff-add" : line.startsWith("-") ? "diff-remove" : ""}>{line}{"\n"}</span>)}
        </pre>}
      </div>
    </div>
  );
}
