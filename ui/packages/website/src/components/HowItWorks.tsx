const steps = [
  {
    title: "Queue work",
    description: "Drop intent into your backlog and trigger a run from CLI or API. UseZombie picks up execution automatically.",
  },
  {
    title: "Agents execute with guardrails",
    description: "Agents plan, implement, and validate with policy controls, sandbox limits, and repo-specific profiles so automation stays inside defined boundaries.",
  },
  {
    title: "Review a validated PR",
    description: "Each pull request includes run replay, validation output, and run quality scoring so reviewers can assess both the change and the run that produced it.",
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
