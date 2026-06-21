import { WakePulse } from "@agentsfleet/design-system";

export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="app-glow-surface min-h-screen flex flex-col items-center justify-center gap-8 bg-background p-6" data-glow="auth">
      <div className="flex items-center gap-2">
        <WakePulse
          live
          className="inline-block w-3 h-3 rounded-full bg-pulse"
          aria-hidden="true"
        />
        <span className="font-mono text-sm font-medium tracking-tight text-foreground">
          agentsfleet
        </span>
      </div>
      {children}
    </div>
  );
}
