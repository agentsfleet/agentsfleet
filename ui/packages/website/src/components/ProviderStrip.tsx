const surfaces = [
  "Anthropic",
  "OpenAI",
  "Fireworks · Kimi K2",
  "Together",
  "Groq",
  "Moonshot",
];

export default function ProviderStrip() {
  return (
    <div className="provider-strip">
      <span className="label">Bring your own model</span>
      <div className="providers">
        {surfaces.map((name) => (
          <span key={name} className="provider">{name}</span>
        ))}
      </div>
    </div>
  );
}
