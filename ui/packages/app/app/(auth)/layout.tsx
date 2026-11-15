import { CheckCircleIcon } from "@phosphor-icons/react/ssr";
import { BrandMark, PageTitle } from "@agentsfleet/design-system";

const STEPS = ["Describe the result you need", "Review the setup and access", "Follow the work and review the results"] as const;

export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return <main className="ph-no-capture ph-no-record app-surface flex min-h-dvh flex-col bg-background" data-surface="auth">
    <div className="flex items-center gap-sm px-xl py-lg sm:px-3xl">
      <BrandMark className="size-6" /><span className="font-medium tracking-tight">agentsfleet</span>
    </div>
    <div className="mx-auto grid w-full max-w-content flex-1 items-center gap-3xl px-lg py-2xl lg:grid-cols-2 lg:gap-5xl lg:px-3xl">
      <div className="content-entrance hidden space-y-2xl lg:block">
        <div className="inline-flex rounded-lg border border-border bg-card p-xl"><BrandMark className="size-14 text-pulse" /></div>
        <div className="max-w-narrow space-y-lg"><PageTitle>Put your agents to work.</PageTitle>
          <p className="text-body-lg text-muted-foreground">Give your fleet a task, connect the tools it needs, and keep the work and results in one place.</p></div>
        <div className="space-y-lg">{STEPS.map((step) => <div key={step} className="flex items-center gap-md text-body-sm text-muted-foreground"><CheckCircleIcon size={16} className="shrink-0 text-pulse" aria-hidden="true" />{step}</div>)}</div>
        <p className="text-label text-muted-foreground">Your tools. Your models. Your direction.</p>
      </div>
      <div className="flex min-w-0 flex-col items-center justify-center gap-xl">{children}<p className="text-center text-label text-muted-foreground">From a task in mind to work you can review.</p></div>
    </div>
  </main>;
}
