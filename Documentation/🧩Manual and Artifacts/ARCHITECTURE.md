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
| [`SRC/src/lib.rs`](../../SRC/src/lib.rs) | `run_browse_agent`, `describe` (the RTSP `DESCRIBE`), `say` / `restate`, `ravenna_sighting` |
| [`SRC/src/sdp.rs`](../../SRC/src/sdp.rs) | `parse` → `StreamInfo`: codec, rate, channels, destination, RTP port, ptime, clock domain, refclk, direction. Also the library DANTE and SAP use |
| [`SRC/src/self_test.rs`](../../SRC/src/self_test.rs) | the self test the runner serves |
| [`SRC/config.ini`](../../SRC/config.ini) | `[ravenna]`: `enabled` and the presence pair (departures only) |
| [`SRC/gui/`](../../SRC/gui/manifest.json) | the `ravenna` console family and its panel |

## Data flow

1. **Browse** `_ravenna._tcp` and `_rtsp._tcp` with mdns-sd (`SERVICE_TYPES`).
2. **Describe.** On each resolution, `describe` sends `DESCRIBE rtsp://<ip>:<port>/by-name/<session>`
   (4 s timeout) and returns the SDP body.
3. **Parse and file.** `sdp::parse`; a non-audio SDP is dropped. The stream is filed under its host,
   its 13 `STREAM_KEYS` published unretained and cached in `fields`; `presence::seen` keeps the
   host's retained presence.
4. **Restate** the whole `fields` cache every 30 s.
5. **Depart.** `ServiceRemoved` drops that session's topics from `fields`; when the host has no
   session left, `presence::gone`.

The runner owns the retained `config`, `incoming/state`, `incoming/api`, the self test and the
presence tracker.
