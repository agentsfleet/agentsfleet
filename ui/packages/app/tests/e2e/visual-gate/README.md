# Interface visual gate

Run from a clean app checkout. The proposal is a separate immutable input;
its approved SHA-256 is checked at capture start and end. These tools do not
replace the repository's native Make gates or authenticated acceptance.

```sh
bun tests/e2e/visual-gate/reference.ts /absolute/proposal.html /absolute/results [chromium-executable]
bun tests/e2e/visual-gate/capture.ts /absolute/results [chromium-executable] /absolute/proposal.html
python3 tests/e2e/visual-gate/report.py /absolute/results
```

The report requires Pillow and FFmpeg. Playwright can use its installed browser
or the supplied Chromium executable. Each run uses a fresh results directory.
No ZIP is produced.

Capture modes are proposal-string parity and realistic labelled samples. Both
use production components and read-only fixture transports. Missing backend
facts remain unavailable. Long secret names have spaced and unbroken variants.
The matrix includes all four widths, both themes, touch at 390px, short composer
focus, toasts without the review bar, and labelled loader boxes.

The report records each measured property separately and leaves checks without
evidence as NOT RUN. It compares reference values from the same run, records
all four corner radii, and hashes every output file. Complete the independent
visual review and authenticated evidence before assigning the final verdict.

The owner's gate and accepted choices live in the repository evidence docs.
