import { fileURLToPath } from "node:url";

// The profiling renderer exposes actualDuration in an otherwise production build.
const built = await Bun.build({
  entrypoints: ["tests/e2e/fixtures/fleet-output-browser-probe.tsx"],
  target: "browser", format: "iife", minify: true,
  define: {"process.env.NODE_ENV": '"production"', "process.env.NEXT_PUBLIC_API_URL": '"https://api-test.agentsfleet.net"'},
  plugins: [{name: "production-react-profiling", setup(build) {
    build.onResolve({filter: /^react-dom\/client$/}, () => ({path: fileURLToPath(import.meta.resolve("react-dom/profiling"))}));
  }}],
});
if (!built.success) throw new AggregateError(built.logs, "Output fixture build failed");
const output = built.outputs[0];
if (!output) throw new Error("Output fixture build produced no JavaScript");
process.stdout.write(await output.text());
