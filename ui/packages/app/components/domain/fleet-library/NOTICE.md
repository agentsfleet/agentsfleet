# Vendor marks

`vendor-marks.ts` carries eight brand marks, extracted from
[Simple Icons](https://github.com/simple-icons/simple-icons) and committed to
this repository rather than installed as a dependency.

## Licence

The Simple Icons **collection** is released under
[CC0-1.0](https://creativecommons.org/publicdomain/zero/1.0/), which is what
permits copying the path data here.

The **marks themselves are trademarks of their respective owners** and are not
covered by that licence. They are reproduced here for one purpose: identifying
which third-party service a Fleet library entry requires a credential for. That
is nominative use — naming the thing being referred to. It asserts no
affiliation with, sponsorship by, or endorsement from any of them.

A rights holder asking for their mark to be removed should have it removed:
delete its entry from `VENDOR_MARKS` and the credential falls back to the
neutral glyph with no other change. Simple Icons dropped its own Slack mark on
exactly such a request, which is why `slack` is absent from the map despite
appearing in this product's fixtures.

## Marks vendored

| Credential | Mark |
|---|---|
| `github` | GitHub |
| `elastic` | Elastic |
| `grafana` | Grafana |
| `jira` | Jira |
| `fly` | Fly.io |
| `upstash` | Upstash |
| `zoho` | Zoho |
| `linear` | Linear |

## Refreshing

The set is deliberately small — the providers this product's bundles and
connector catalogue actually name. It is not meant to grow toward Simple Icons'
full three thousand; an unrecognised provider is a supported outcome, not a gap.

To refresh a path after an upstream redraw:

```bash
npm pack simple-icons@<version>
tar -xzf simple-icons-<version>.tgz
# each icon is package/icons/<slug>.svg — one <path d="…"> on a 24x24 viewBox
```

Copy the `d` attribute into `VENDOR_MARKS`. Nothing else in the SVG is used:
the marks inherit `currentColor` and their size from the class `VendorMark`
applies.
