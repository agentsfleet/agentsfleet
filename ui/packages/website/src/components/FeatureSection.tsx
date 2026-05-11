import { Card } from "@usezombie/design-system";

type Props = {
  number: string;
  title: string;
  description: string;
};

/*
 * FeatureSection — single capability tile. Mono number eyebrow + mono
 * title + sans body. Borders > shadows per DESIGN_SYSTEM.md §Layout.
 */
export default function FeatureSection({ number, title, description }: Props) {
  return (
    <Card className="flex flex-col gap-3" data-testid="feature-section">
      <span className="font-mono text-eyebrow uppercase tracking-eyebrow text-text-subtle">
        {number}
      </span>
      <h3 className="font-mono text-heading leading-heading text-text font-medium m-0">
        {title}
      </h3>
      <p className="font-sans text-body leading-body text-text-muted m-0">
        {description}
      </p>
    </Card>
  );
}
