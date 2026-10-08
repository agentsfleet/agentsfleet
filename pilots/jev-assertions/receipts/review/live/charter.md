# Approved measurement review

- Scope: local command-line accounting and exact offline replay in the owned pilot worktree.
- Source: M216_001 §2 and §3, the owner-approved twenty-request ceiling, and README.md review/accounting protocol.
- Inputs: committed freeze 7c1117c91f17918519b550dd1b27c5e04980bafe53313adac726f175bf015caa; unchanged approved runner 4d0644fd93a46bac785f5282392310f24634092bd4843a9189acd1b02875f9e8; actual approval, reservations and two retained live receipts.
- Revision: 908d3cde491fcd31efb6f34cd9eb700def98a792 plus measured receipt/report/spec changes.
- Authority: repository-required gstack review; write only pilot evidence, summary and replay receipts.
- Tools: Python 3, pinned orly 0.13.0 and the installed gstack evidence recorder under Bun 1.4.2.
- Isolation: own worktree; own private replay cache; no new provider requests, datastore resets or product mutations.
- Budget: one five-minute smoke window, at most twelve probes; finite command deadlines.
- Required success: summarize all twenty real attempts with reserved/reported requests 20, unknown usage zero, native agreement 14, native seeded findings 12, native false alarms 3, actionable findings 7 and independent comparison unmeasured.
- Required edge: exact replay returns all twenty retained native answers with zero requests; live receipts and reservation ledger remain byte-identical.
- Proof reuse: previously recorded 25 runner checks and forty fixture executions retain identical frozen source/test/command inputs. They are Section evidence, not new review captures or repository-wide suite claims.
- Exit: required current accounting and replay observations match the frozen evidence; report independent reviewer benefit and pre-Pull Request boundary suites as unmeasured/pending.
- Cleanup: owned subprocesses exit; retain all safe review evidence and private replay state.
