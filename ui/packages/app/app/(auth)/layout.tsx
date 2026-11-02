import { ArrowUpRightIcon, CircleCheckIcon } from "lucide-react";
import { BrandMark, PageTitle } from "@agentsfleet/design-system";

const STEPS = ["Describe the work", "Review the plan and access", "Let your fleet take it from here"] as const;

export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return <main className="ph-no-capture ph-no-record app-surface flex min-h-dvh flex-col bg-background" data-surface="auth">
    <div className="flex items-center gap-sm px-xl py-lg sm:px-3xl">
      <BrandMark className="size-6" /><span className="font-medium tracking-tight">agentsfleet</span>
    </div>
    <div className="mx-auto grid w-full max-w-content flex-1 items-center gap-3xl px-lg py-2xl lg:grid-cols-2 lg:gap-5xl lg:px-3xl">
      <div className="content-entrance hidden space-y-2xl lg:block">
        <div className="signal-halo inline-flex rounded-lg bg-card p-xl"><BrandMark className="size-14 text-pulse" /></div>
        <div className="max-w-narrow space-y-lg"><PageTitle>Good work starts with a conversation.</PageTitle>
          <p className="text-body-lg text-muted-foreground">Turn the work on your mind into a fleet that keeps it moving.</p></div>
        <div className="space-y-lg">{STEPS.map((step) => <div key={step} className="flex items-center gap-md text-body-sm text-muted-foreground"><CircleCheckIcon size={16} className="text-pulse" />{step}</div>)}</div>
        <p className="flex items-center gap-sm text-label text-muted-foreground">Your direction. Your approval.<ArrowUpRightIcon size={14} /></p>
      </div>
      <div className="flex min-w-0 flex-col items-center justify-center gap-xl">{children}<p className="text-label text-muted-foreground">A little direction. A lot off your plate.</p></div>
    </div>
  </main>;
}
