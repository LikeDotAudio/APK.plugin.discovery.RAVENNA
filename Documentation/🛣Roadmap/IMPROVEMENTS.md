# 🛣 Improvements — PLUGIN.RAVENNA (from the 2026-10-30 architecture audit)

What this repo should change to meet the standard, most valuable first. Source: [🔍Audits/ARCHITECTURE-AUDIT.md](../%F0%9F%94%8DAudits/ARCHITECTURE-AUDIT.md).
← [Roadmap](INDEX.md)

| # | Improvement | Category | Effort (S/M/L) | Evidence |
|---|---|---|---|---|
| 1 | Needs owner: push PLUGIN.RAVENNA and publish a tag for crate 0.2.0 (e.g. `ravenna-v0.2.0`), then repin SAP / DANTE / AES67 / PTP from `rev` to that tag. | Change management | S | `SRC/Cargo.toml` `version = "0.2.0"`; consumers' `apkaudio-ravenna = { …, rev = … }` |

Everything else from the audit is delivered — see [🌍Publishing/DELIVERY.md](../%F0%9F%8C%8DPublishing/DELIVERY.md#delivered-from-the-2026-10-30-audit).
