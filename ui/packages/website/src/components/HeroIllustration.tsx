export default function HeroIllustration() {
  const colors = {
    bucketStart: "var(--z-illustration-bucket-start)",
    bucketEnd: "var(--z-illustration-bucket-end)",
    skinStart: "var(--z-illustration-skin-start)",
    skinEnd: "var(--z-illustration-skin-end)",
    halo: "var(--z-illustration-halo)",
    strap: "var(--z-illustration-strap)",
    eye: "var(--z-illustration-eye)",
    mouth: "var(--z-illustration-mouth)",
    arm: "var(--z-illustration-arm)",
  };

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
            <stop offset="0%" stopColor={colors.bucketStart} />
            <stop offset="100%" stopColor={colors.bucketEnd} />
          </linearGradient>
          <linearGradient id="skin" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor={colors.skinStart} />
            <stop offset="100%" stopColor={colors.skinEnd} />
          </linearGradient>
        </defs>

        <circle cx="160" cy="160" r="124" fill={colors.halo} />
        <rect x="104" y="48" width="112" height="58" rx="12" fill="url(#bucket)" />
        <path d="M112 66h96" stroke={colors.strap} strokeWidth="6" strokeLinecap="round" />
        <ellipse cx="160" cy="154" rx="70" ry="84" fill="url(#skin)" />
        <circle cx="136" cy="144" r="10" fill={colors.eye} />
        <circle cx="184" cy="144" r="10" fill={colors.eye} />
        <path d="M134 190c9 11 43 11 52 0" stroke={colors.mouth} strokeWidth="8" strokeLinecap="round" />
        <rect x="78" y="214" width="66" height="22" rx="11" fill={colors.arm} transform="rotate(-16 78 214)" />
        <rect x="176" y="214" width="66" height="22" rx="11" fill={colors.arm} transform="rotate(16 176 214)" />
      </svg>
      <div className="hero-illustration-caption">undead operator mark</div>
    </aside>
  );
}
