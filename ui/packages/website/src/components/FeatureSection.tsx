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
      <span className="font-mono text-[12px] uppercase tracking-[0.08em] text-text-subtle">
        {number}
      </span>
      <h3 className="font-mono text-[18px] leading-[1.3] tracking-[-0.01em] text-text font-medium m-0">
        {title}
      </h3>
      <p className="font-sans text-[15px] leading-[1.55] text-text-muted m-0">
        {description}
      </p>
    </Card>
  );
}
