export default function HeroIllustration() {
  return (
    <aside className="hero-illustration" aria-hidden="true">
      <div className="hero-illustration-grid" />
      <svg
        className="hero-zombie-mark"
        viewBox="0 0 320 320"
        role="presentation"
      >
        <defs>
          <linearGradient id="bucket" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#8892a0" />
            <stop offset="100%" stopColor="#5f6977" />
          </linearGradient>
          <linearGradient id="skin" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#66a093" />
            <stop offset="100%" stopColor="#4f7b71" />
          </linearGradient>
        </defs>

        <circle cx="160" cy="160" r="124" fill="rgba(255, 140, 66, 0.1)" />
        <rect x="104" y="48" width="112" height="58" rx="12" fill="url(#bucket)" />
        <path d="M112 66h96" stroke="#aeb7c4" strokeWidth="6" strokeLinecap="round" />
        <ellipse cx="160" cy="154" rx="70" ry="84" fill="url(#skin)" />
        <circle cx="136" cy="144" r="10" fill="#111" />
        <circle cx="184" cy="144" r="10" fill="#111" />
        <path d="M134 190c9 11 43 11 52 0" stroke="#1a2420" strokeWidth="8" strokeLinecap="round" />
        <rect x="78" y="214" width="66" height="22" rx="11" fill="#55786f" transform="rotate(-16 78 214)" />
        <rect x="176" y="214" width="66" height="22" rx="11" fill="#55786f" transform="rotate(16 176 214)" />
      </svg>
      <div className="hero-illustration-caption">undead operator mark</div>
    </aside>
  );
}
