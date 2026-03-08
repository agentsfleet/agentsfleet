const surfaces = [
  "GitHub",
  "CLI",
  "API",
];

export default function ProviderStrip() {
  return (
    <div className="provider-strip">
      <span className="label">Where UseZombie works</span>
      <div className="providers">
        {surfaces.map((name) => (
          <span key={name} className="provider">{name}</span>
        ))}
      </div>
    </div>
  );
}
