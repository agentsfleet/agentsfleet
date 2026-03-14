const steps = [
  {
    title: "Queue work",
    description: "Drop intent into your backlog and trigger a run from CLI or API. UseZombie picks up execution automatically.",
  },
  {
    title: "Agents execute with guardrails",
    description: "Echo plans, Scout patches, and Warden validates with policy controls today, with stronger sandbox resource governance shipping in paid plans.",
  },
  {
    title: "Review a validated PR",
    description: "A pull request opens with run replay, validation output, and the score-based quality context that helps teams improve the next run.",
  },
];

export default function HowItWorks() {
  return (
    <div className="section-gap">
      <p className="eyebrow">Why UseZombie</p>
      <h2>From queued intent to validated pull requests.</h2>
      <div className="how-steps">
        {steps.map((step) => (
          <div key={step.title} className="how-step">
            <h3>{step.title}</h3>
            <p>{step.description}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
