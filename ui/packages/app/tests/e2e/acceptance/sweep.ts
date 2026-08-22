import * as fs from "node:fs";
import { loadWorktreeEnv } from "./fixtures/env-loader";
loadWorktreeEnv();
import { signSvix, newMsgId } from "./fixtures/svix";

const APPLY = process.argv.includes("--apply");
const CK = process.env.CLERK_SECRET_KEY!;
const SECRET = process.env.CLERK_WEBHOOK_SECRET!;
const API = process.env.NEXT_PUBLIC_API_URL!;
const rows = fs.readFileSync(process.argv[2]!, "utf8").trim().split("\n")
  .map((l) => l.split("|")).filter((p) => p[0]?.startsWith("user_"));

const orphans: string[][] = [];
const live: string[][] = [];
for (const [id, email] of rows) {
  const r = await fetch(`https://api.clerk.com/v1/users/${id}`, { headers: { Authorization: `Bearer ${CK}` } });
  (r.status === 404 ? orphans : live).push([id!, email ?? ""]);
  if (r.status !== 404 && r.status !== 200) console.log("  ?? unexpected", r.status, id);
  await new Promise((s) => setTimeout(s, 60));
}
console.log(`live in Clerk (KEEP): ${live.length}`);
for (const [, e] of live) console.log("   keep:", e);
console.log(`orphaned (Clerk 404): ${orphans.length}`);

if (!APPLY) { console.log("\nDRY RUN — no writes. Re-run with --apply."); process.exit(0); }

let ok = 0, failed = 0;
for (const [id] of orphans) {
  const body = JSON.stringify({ type: "user.deleted", data: { id, deleted: true } });
  const h = signSvix(SECRET, newMsgId(), body);
  const res = await fetch(`${API}/v1/auth/identity-events/clerk`, {
    method: "POST", headers: { ...h, "Content-Type": "application/json" }, body,
  });
  if (res.ok) ok += 1; else { failed += 1; console.log("  FAIL", id, res.status, (await res.text()).slice(0, 120)); }
  await new Promise((s) => setTimeout(s, 80));
}
console.log(`purged: ${ok} · failed: ${failed}`);
