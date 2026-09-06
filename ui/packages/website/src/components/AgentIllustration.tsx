export function AgentIllustration({ className }: { className: string }) {
  return (
      <svg className={className} viewBox="0 0 200 160" aria-hidden="true">
        <path className="incident-wire" d="M0 80H42M158 80H200" pathLength="1" />
        <rect x="44" y="29" width="112" height="104" rx="16" fill="var(--surface-2)" stroke="var(--border-strong)" />
        <rect x="59" y="45" width="82" height="57" rx="9" fill="var(--pulse)" />
        <path d="M82 64v18m36-18v18" stroke="var(--on-pulse)" strokeWidth="7" strokeLinecap="round" />
        <path d="M83 117h34M100 29V16" stroke="var(--text-muted)" strokeWidth="3" strokeLinecap="round" />
        <circle cx="100" cy="12" r="5" fill="var(--pulse)" />
      </svg>
  );
}
