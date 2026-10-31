# PLUGIN.RAVENNA architecture

How to use it, and how it works inside. Every topic key is listed in the [repo README](../../README.md).

← [Manual and Artifacts](INDEX.md)

## Use

```sh
# from PODS/POD.THICK/DISCOVERY.Engine/SRC (the cargo workspace)
cargo test -p apkaudio-ravenna
COMPOSE_PROFILES=discovery docker compose -f PODS/POD.THICK/PLUGINS/PLUGIN.RAVENNA/Docker/docker-compose.yml up -d --build
```

`apk-ravenna` takes no arguments. Read streams at
`APK.audio/System/Protocols/ravenna/Device/{host}/{session}/{key}` (within 30 s of subscribing) and
liveness at `APK.audio/Discovery/ravenna/+`.

## Entry points

| File | What |
|---|---|
| [`SRC/src/bin/apk-ravenna.rs`](../../SRC/src/bin/apk-ravenna.rs) | `apk_plugin_runner::main!(token = "ravenna", run = [run_browse_agent])` |
| [`SRC/src/lib.rs`](../../SRC/src/lib.rs) | the crate map: the shared readers (always) and, behind feature `agent`, the agent and self test |
| [`SRC/src/agent.rs`](../../SRC/src/agent.rs) | the two adapters (`heard_from`: mDNS event → `Heard`; `Rtsp`: the `Describe` that runs `describe`, the RTSP `DESCRIBE`), `Browse` (pure state: `hear` → `Effect`s), `run_browse_agent` (runner `Session`, `unkept::say`/`restate`), `ravenna_sighting` |
| [`SRC/src/sdp.rs`](../../SRC/src/sdp.rs) | `parse` → `StreamInfo` (discovery); `parse_stream` / `generate` → `StreamDesc` (strict, AES67 / ST 2110-30); `corpus()` over [`SRC/vectors/sdp/`](../../SRC/vectors/sdp) |
| [`SRC/src/sap.rs`](../../SRC/src/sap.rs), [`SRC/src/ptp.rs`](../../SRC/src/ptp.rs), [`SRC/src/rtsp.rs`](../../SRC/src/rtsp.rs) | the shared SAP header, PTPv2 message and RTSP `/by-name/` readers (see the README's table of who reads what) |
| [`SRC/src/self_test.rs`](../../SRC/src/self_test.rs) | the self test the runner serves |
| [`SRC/config.ini`](../../SRC/config.ini) | `[ravenna]`: `enabled` and the presence pair (departures only) |
| [`SRC/gui/`](../../SRC/gui/manifest.json) | the `ravenna` console family and its panel |

## Data flow

1. **Browse** `_ravenna._tcp` and `_rtsp._tcp` with mdns-sd (`SERVICE_TYPES`); each event becomes a
   `Heard` (`heard_from`) and is folded in by `Browse::hear`, whose effects the loop carries out.
2. **Describe.** On each resolution, `describe` sends `DESCRIBE rtsp://<ip>:<port>/by-name/<session>`
   (`rtsp_timeout_s`, 4 s) and returns the SDP body.
3. **Parse and file.** `sdp::parse`; a non-audio SDP is dropped. The stream is filed under its host,
   its 13 `STREAM_KEYS` published unretained and cached in `fields`; `presence::seen` keeps the
   host's retained presence.
4. **Restate** the whole `fields` cache every `restate_s` (30 s). Both are `config.ini` keys read by `Settings::from_ini`.
5. **Depart.** `ServiceRemoved` drops that session's topics from `fields`; when the host has no
   session left, `presence::gone`.

The runner owns the retained `config`, `incoming/state`, `incoming/api`, the self test and the
presence tracker.
