# M249 visual decisions

On Nov 13, 2026, Indy answered “Accept all nine recommendations” to the
following numbered request. These decisions apply to the next candidate
commit that implements them; the run records that revision and source digest.

| Item | Accepted recommendation | Checks |
| --- | --- | --- |
| D1 | Changes flap only on Activity. | I3, J1 |
| D2 | Phone composer scrolls with the page. | K2, K3 |
| D3 | Create model footer buttons are 44px at every width. | E1, E2 |
| D4 | Create model on phone uses 20px padding and a pinned footer with divider. | F2, F4 |
| D5 | Loader uses the same base colour with animation and reduced motion in both themes, with a darker sweep. | L5 |
| D6 | Activity/Working header reflects real available status; do not invent “Ready to review”. | G3, J4 |
| D7 | Archived Summary adds Status explaining it cannot resume. | J5 |
| D8 | Keep Usage phone selects at 16px. | C4 |
| D9 | Restore compact 36px wake-up strip and desktop Team row controls. | E3, I4 |

The exact answer approves only these choices. It does not approve a visual
result, missing evidence, a failed check, or opening a PR before the gates pass.

## Owner review delivery, Nov 13

Indy wrote: “i dont need you capture now”, followed by “internal capture is needed if you need it, indy doesnt need it.” He will eyeball “in the deploy” or “preview in PR”.

Stop the full capture/delivery loop. Fix the findings in the matrix and use targeted internal comparisons when needed. No ZIP or gallery delivery is requested. This changes the evidence delivery; it does not approve any unresolved visual defect or waive repository tests.
