# 🔍 Architecture audit — PLUGIN.RAVENNA (2026-10-30)

Audited against the APK.audio multi-repo architecture standard: coupling, boundaries, duplication, contracts.
← [Audits](INDEX.md) · Improvements: [🛣Roadmap/IMPROVEMENTS.md](../%F0%9F%9B%A3Roadmap/IMPROVEMENTS.md)

## Bounded context
RAVENNA / AES67 stream discovery: mDNS browse (`_ravenna._tcp`, `_rtsp._tcp`) → one RTSP DESCRIBE → SDP parse → per-stream fields and a host presence on the bus. Also the owner of the SDP parser that PLUGIN.DANTE and PLUGIN.SAP link. Rust crate `apkaudio-ravenna` (912 lines), bin `apk-ravenna`. Read in full: `SRC/Cargo.toml`, `SRC/config.ini`, `SRC/gui/manifest.json`, compose, `src/lib.rs` header and agent loop, `src/sdp.rs` exports; sampled `self_test.rs`.

## Context map
| Talks to | How (topic / endpoint / file / import) | Relationship |
|---|---|---|
| APK.plugin.SDK | `apk-plugin-runner`, `apkaudio-contracts` @ `tag = "sdk-v0.5"`; `plugin-ci.yml@main` | Shared Kernel (versioned) |
| DISCOVERY.Engine | compose `extends: ../../../DISCOVERY.Engine/Docker/plugin-base.yml` (`Docker/docker-compose.yml:18-19`); workspace member; image `apk-plugins:local` | Customer–Supplier |
| PLUGIN.DANTE, PLUGIN.SAP | depend on this crate's `sdp` module by git `rev = "55ff760…"` (their `SRC/Cargo.toml`) | Published Language / Shared Kernel (this repo supplies) |
| RAVENNA nodes | mDNS (`mdns-sd`), RTSP `DESCRIBE rtsp://<ip>:<port>/by-name/<session>` (`src/lib.rs:148-151`) | Anti-Corruption Layer (`sdp.rs`) |
| MQTT bus | `APK.audio/System/Protocols/ravenna/Device/{host}/{session}/{key}` (QoS 0, unretained, restated); runner presence `Discovery/ravenna/{host}`; panels via `gui/manifest.json` | Published Language |

## Scorecard
| Category | Score (0–3) | Finding |
|---|---|---|
| Coupling & Cohesion | 3 | Bus + SDK tags at runtime; consumers take only the readers (`default-features = false`), not the agent. The build's reach into DISCOVERY.Engine (base file, workspace) is the house pattern, kept by design (PRINCIPLES.md). |
| Boundary Integrity | 3 | `sdp.rs` / `sap.rs` / `ptp.rs` / `rtsp.rs` are clean translations; `mdns-sd` and the RTSP `TcpStream` sit behind two adapters (`heard_from`, `Rtsp: Describe`) in `agent.rs`, and every topic comes from `topics::protocols`. |
| Duplication Control | 3 | One SDP, SAP, PTP and RTSP-path reader for AES67, SAP, DANTE, PTP and this agent; session and unkept publish from the runner. |
| Testing & Contracts | 3 | 44 tests: the readers, a fixture corpus (`SRC/vectors/sdp/`, `sdp::corpus()`) that SAP and AES67 re-run against the rev they pin, the browse driven by canned announcements, the unkept policy. |
| Change management | 3 | Crate `0.2.0` with a stated MINOR rule, published as tag `ravenna-v0.2.0`; SAP / DANTE / AES67 / PTP pin that tag; CI on the SDK tag. |
Score key: 3 meets the standard · 2 minor gaps · 1 significant gaps · 0 absent / violated.

## Findings
1. **Good — discovery requires evidence.** A service is filed only once its SDP carries `m=audio`; port 554 alone is not a RAVENNA device (`Browse::hear`, test `video_and_silent_services_are_not_filed`).
2. **Good — unkept announcements.** Every stream topic is QoS 0, retain clear, restated on a timer without re-fetching SDP, through the runner's `unkept::say`/`restate`; a test asserts no direct publish exists (`nothing_this_agent_publishes_is_kept`).

Delivered items are recorded in [🌍Publishing/DELIVERY.md](../%F0%9F%8C%8DPublishing/DELIVERY.md#delivered-from-the-2026-10-30-audit).
