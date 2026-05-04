const steps = [
  {
    title: "A trigger arrives",
    description: "A GitHub Actions deploy fails, a cron fires, or you run `zombiectl steer`. Each lands on the event stream with actor provenance: webhook:github, cron:<schedule>, steer:<user>.",
  },
  {
    title: "The agent gathers evidence",
    description: "It calls the tools TRIGGER.md allow-lists — http_request, memory_store, cron_add. Secrets substitute at the sandbox boundary; the model sees placeholders, never raw bytes.",
  },
  {
    title: "Diagnosis posts; the run is auditable",
    description: "Slack receives the evidenced diagnosis. Every event is on core.zombie_events with actor and timestamp. `zombiectl steer {id}` picks the conversation up later.",
  },
];

export default function HowItWorks() {
  return (
    <div className="section-gap">
      <p className="eyebrow">How it works</p>
      <h2>From trigger to evidenced diagnosis, durably.</h2>
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
