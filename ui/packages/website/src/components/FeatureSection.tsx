import { Card } from "@agentsfleet/design-system";

type Props = {
  number: string;
  title: string;
  description: string;
  compact?: boolean;
};

/*
 * FeatureSection — single capability tile. Mono number eyebrow + mono
 * title + sans body. Borders > shadows per DESIGN_SYSTEM.md §Layout.
 */
export default function FeatureSection({ number, title, description, compact }: Props) {
  return (
    <Card
      className={compact ? "flex flex-col gap-2" : "flex flex-col gap-3"}
      data-testid="feature-section"
    >
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        {number}
      </span>
      {compact ? (
        <h4 className="font-mono text-heading leading-heading text-text font-medium m-0">
          {title}
        </h4>
      ) : (
        <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
          {title}
        </h3>
      )}
      <p
        className={
          compact
            ? "font-sans text-body-sm leading-body text-text-muted m-0"
            : "font-sans text-body leading-body text-text-muted m-0"
        }
      >
        {description}
      </p>
    </Card>
  );
}
