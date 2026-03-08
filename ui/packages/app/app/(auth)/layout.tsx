export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="auth-shell">
      <div className="auth-brand">
        <span className="auth-logo">UseZombie</span>
        <span className="auth-tag">Mission Control</span>
      </div>
      {children}
      <style>{`
        .auth-shell {
          min-height: 100vh;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 2rem;
          background: var(--z-bg-0);
        }
        .auth-brand {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 0.25rem;
        }
        .auth-logo {
          font-size: 1.5rem;
          font-weight: 700;
          letter-spacing: 0.03em;
        }
        .auth-tag {
          font-family: var(--z-font-mono);
          font-size: 0.72rem;
          color: var(--z-amber);
          text-transform: uppercase;
          letter-spacing: 0.08em;
        }
      `}</style>
    </div>
  );
}
