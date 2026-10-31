# PLUGIN.RAVENNA delivery

← [Publishing](INDEX.md) · [Repo README](../../README.md)

## Built and tested

The crate `apkaudio-ravenna` (`SRC/`) is a member of the plugin workspace
`PODS/POD.THICK/DISCOVERY.Engine/SRC`: `cargo test -p apkaudio-ravenna` there. The binary `apk-ravenna` is
compiled with every other plugin into the shared image `apk-plugins:local` by
[`DISCOVERY.Engine/Docker/Dockerfile`](../../../../DISCOVERY.Engine/Docker/Dockerfile) (target
`plugins`; the build context is the APK.audio repo root). This repo has no Dockerfile.

## Shipped

As the container `Plugin-RAVENNA` from [`Docker/docker-compose.yml`](../../Docker/docker-compose.yml):
service `ravenna`, profile `discovery`, host network (mDNS does not cross a bridge), `extends: plugin-host` in
[`plugin-base.yml`](../../../../DISCOVERY.Engine/Docker/plugin-base.yml) (image, `cap_drop: [ALL]`,
read-only root, broker variables, log and OUI-cache volumes, the runner's healthcheck).
`DISCOVERY.Engine/Docker/docker-compose.yml` `include:`s this file into the compose project
`apk-audio`, so it starts with the other plugins on a node whose role list names `discovery`.

It lands on the node's broker, under `APK.audio/System/Protocols/ravenna/…` (unretained, restated every 30 s) and `APK.audio/Discovery/ravenna/…` (presence, retained), and in the dashboards as the `ravenna` console family. There is no live URL. The repository
(https://github.com/LikeDotAudio/APK.plugin.discovery.RAVENNA.git) is pushed with every other submodule by `.apk.scripts/deploy_all.py` at the APK.audio
root.

## Not automated

* CI is the SDK's reusable `plugin-ci.yml@sdk-v0.5` on every push (pinned to the same tag as the
  dependencies); the monorepo's own CI builds nothing here, on purpose.
* No release artefact: the image is built locally on each node (`up -d --build`). The crate is
  versioned (`0.2.0`); tags are published by the owner.
* `PLUGIN.SAP`, `PLUGIN.DANTE`, `PLUGIN.AES67` and `PLUGIN.PTP` take this crate's readers by git
  `rev` with `default-features = false`; in the monorepo the engine `[patch]` builds them from this
  checkout, so a change to a reader reaches them when the plugin image is rebuilt.

## Delivered from the 2026-10-30 audit

The crate is one plugin (`Plugin-RAVENNA`, in the shared plugin image) and the library of shared
AoIP readers that PLUGIN.AES67, PLUGIN.SAP, PLUGIN.DANTE and PLUGIN.PTP build against. Verified with
`cargo test -p apkaudio-ravenna` (44 passed) and `--no-default-features` (the readers alone) in the
DISCOVERY.Engine workspace, plus each consumer's own `cargo test -p`.

- **One home for SDP / SAP / PTP parsing** (audit #1, and AES67 #1) — `src/sdp.rs` now holds both the discovery reader (`parse` → `StreamInfo`) and AES67's strict reader/writer (`parse_stream` / `generate` / `StreamDesc` / `Encoding` / `Profile` / `conformance_level` / `channel_order`) over one line grammar; `src/sap.rs` is PLUGIN.SAP's RFC 2974 reader plus AES67's `encode` / `msg_id_hash`; `src/ptp.rs` is PLUGIN.PTP's PTPv2/gPTP parser plus `encode_delay_req`, `Timestamp::as_nanos`, `correction_whole_ns`, `source_port`/`requesting_port` for AES67's slave; `src/rtsp.rs` is the `/by-name/` encoding both ends spell. Copies deleted in AES67 (`sdp.rs`, `sap.rs` header, `clock.rs` wire format, `ravenna.rs` url codec), SAP (`sap.rs`) and PTP (`message.rs`). Kept in this repository as the brief asked (no new repo).
- **Library without the plugin** (finding 5) — features: `agent` (default: browse agent, self test, `mdns-sd`, `rumqttc`, runner, contracts) and `serde`; consumers take `default-features = false` and get std-only readers. Crate `0.1.0` → `0.2.0` with a stated MINOR rule (`SRC/Cargo.toml`). Consumers re-pinned to this commit. Tag: Needs owner (still in the audit).
- **SDP fixture corpus shared with consumers** (#3) — `SRC/vectors/sdp/` (Merging RTSP reply, Dante AES67, ST 2110-30 level B, mono L16, AM824, an IP camera) with expected parses in `sdp::corpus()`; `the_corpus_parses_as_declared` here, and SAP / AES67 re-run it against the rev they pin.
- **mDNS and RTSP behind adapters** (#4, finding 6) — `src/agent.rs`: `heard_from` (mDNS event → `Heard`), `Describe` trait with `Rtsp` (the TCP `DESCRIBE`), and `Browse::hear` (pure state → `Effect`s). New tests drive the whole browse with canned announcements: `a_canned_audio_announcement_is_filed_under_its_host`, `video_and_silent_services_are_not_filed`, `a_host_goes_with_its_last_session`, `ipv6_only_announcements_are_skipped`.
- **Topic prefix literal ×3** (#2, finding 4) — already one function, `device_topic`, over the contract's `topics::protocols("ravenna", …)` (commit 1fa1dd4, test `device_topics_are_the_contract_protocols_root`); config.ini says why it declares no `topic` key.
- **Runner session and unkept helpers** — the hand-built `rumqttc` client is `Session::new("ravenna", "browse")…spawn_drained()`; the local `say` / `restate` are `apk_plugin_runner::unkept`; the source test now forbids any direct publish.
- **CI pinned** (#5) — `.github/workflows/ci.yml` → `plugin-ci.yml@sdk-v0.5`. Standalone CI needs the next SDK tag (runner `session`/`unkept`, `plugin-volumes.yml`) — publishing it is the owner's.
- **Shared plugin volumes** — the copied `apk-audio-logs` / `mac-lookup-cache` block is now `include: ../../../DISCOVERY.Engine/Docker/plugin-volumes.yml` (`COMPOSE_PROFILES=discovery docker compose … config -q` passes).
- **Identity is the hostname** (finding 3) — written down as the stated fallback for this protocol (mDNS/SDP carry no serial or MAC; never an IP), PRINCIPLES.md.
- **Kept by design: compose `extends` plugin-base.yml and the engine `[patch]` / `members/` link** — the house plugin pattern; reason in PRINCIPLES.md.
- **Tagged and consumed by tag** (IMPROVEMENTS #1 of the first pass) — the coordinator pushed this repository and published `ravenna-v0.2.0` (→ `2695d40`); SAP, DANTE, AES67 and PTP now pin `tag = "ravenna-v0.2.0"`. Verified: a standalone `git archive HEAD` of this repo and of each consumer runs `cargo test` green against the GitHub tags (RAVENNA 44, SAP 13, DANTE 250, AES67 31, PTP 85).
