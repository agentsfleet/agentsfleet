import type { Definition } from "@/lib/api/draft-types";
import { List, ListItem, Section } from "@agentsfleet/design-system";
import { readDecisionPolicy } from "@/lib/api/decision-policy";

type Question = {question: string; criteria: {true: string; false: string}};

/** `h2` when no heading above this one on the page sits at level 2; `h3` beneath one that does. */
export function SetupQuestions({definition, label, heading = "h3"}: {definition: Definition; label?: string; heading?: "h2" | "h3"}) {
  const Heading = heading;
  const result = readDecisionPolicy(definition.config);
  if (result.kind === "absent") return null;
  if (result.kind === "invalid") return <Section aria-label="Success criteria">
    <Heading className="font-semibold">Success criteria unavailable</Heading>
    <p className="text-label leading-label text-muted-foreground">This setup’s success criteria could not be read. Refresh the setup before relying on these criteria.</p>
  </Section>;
  const {completion} = result.policy;
  return <Section className="gap-sm" aria-label="Success criteria">
    {label ? <p className="text-label text-muted-foreground">{label}</p> : null}
    <Heading className="font-semibold">What counts as done?</Heading>
    <p className="text-label leading-label text-muted-foreground">These questions define success for this setup. Change the wording or criteria through setup chat. Each save creates a new revision.</p>
    <List variant="plain" className="space-y-md">
      {completion.statements.map((statement, index) => <ListItem key={statement.id} className="break-words">
        <QuestionDetails number={index + 1} question={statement} />
      </ListItem>)}
    </List>
    <p className="text-eyebrow leading-eyebrow text-muted-foreground">Work already started keeps its saved success criteria.</p>
  </Section>;
}

export function QuestionDetails({number, question}: {number: number; question: Question}) {
  return <div className="space-y-xs text-label leading-label">
    <p className="break-words font-medium">{number}. {question.question}</p>
    <dl className="space-y-xs text-muted-foreground">
      <div className="break-words"><dt className="inline font-medium">Yes: </dt><dd className="inline">{question.criteria.true}</dd></div>
      <div className="break-words"><dt className="inline font-medium">No: </dt><dd className="inline">{question.criteria.false}</dd></div>
    </dl>
  </div>;
}
