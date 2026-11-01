const built = await Bun.build({
  entrypoints: ["tests/e2e/fixtures/chat-browser-probe.tsx"], target: "browser", format: "esm", minify: true,
  define: {"process.env": "{}", "process.env.NODE_ENV": '"production"', "process.env.NEXT_PUBLIC_API_URL": '"https://api-test.agentsfleet.net"'},
});
if (!built.success) throw new AggregateError(built.logs, "Chat fixture build failed");
const output = built.outputs[0];
if (!output) throw new Error("Chat fixture build produced no JavaScript");
process.stdout.write(await output.text());
export {};
