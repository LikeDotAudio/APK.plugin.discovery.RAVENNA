# APK:plugin:RAVENNA

RAVENNA / AES67 browse, one container — and the SDP parser DANTE and SAP share.

| | |
|---|---|
| **Recipe** | `Docker/docker-compose.yml` — no Dockerfile: it runs a binary from the shared plugin image built by `PODS/POD.THICK/DISCOVERY.Engine/Docker/Dockerfile`, extending `plugin-host` in `DISCOVERY.Engine/Docker/plugin-base.yml` |
| **Source** | `SRC/` (this repository — https://github.com/LikeDotAudio/APK.plugin.discovery.RAVENNA.git) — crate `apkaudio-ravenna`, a member of the `DISCOVERY.Engine/SRC` workspace; bin `apk-ravenna` (`apk_plugin_runner::main!(token = "ravenna", run = [run_browse_agent])`) |
| **Container** | `Plugin-RAVENNA`, command `apk-ravenna` |
| **Profile** | `discovery` |
| **Network** | host — mDNS does not cross a bridge |
| **Talks to** | the broker; and each announced node's RTSP port, once per resolution, for one `DESCRIBE` |
| **Bus token** | `ravenna` |

Was a thread in `workers/discovery.rs`. The crate stays a library to
`PLUGIN.DANTE` and `PLUGIN.SAP` for its `sdp` module (`src/sdp.rs`). Moved into
its own container by PLAN-3316.01.

## What it does

`run_browse_agent` (`src/lib.rs`) follows the RAVENNA chain end to end:

1. **mDNS** — browse `_ravenna._tcp` and `_rtsp._tcp` (`SERVICE_TYPES`).
2. **RTSP DESCRIBE** — `rtsp://<ip>:<port>/by-name/<session>`, 4 s timeout.
3. **SDP** — encoding, sample rate, channels, multicast destination, RTP port,
   packet time, PTP clock domain, refclk, direction (`src/sdp.rs`).

A service is filed only once its own SDP carries `m=audio` — port 554 alone is
also every IP camera. Streams are grouped by **hostname**, one device with N
streams; a host keeps every address its sessions came from. Discovery and
description only: no RTP, no subscription, no connection management.

```
APK.audio/System/Protocols/ravenna/Device/{host}/{session}/{key}   ← QoS 0, retain clear
  key ∈ stream format sample_rate channels destination rtp_port ptime_ms
        clock_domain refclk direction status last_online online_since
```

## Nothing this agent publishes is kept

Every discovery topic goes out at **QoS 0 with the retain flag clear** — the one
combination MQTT offers that the broker neither stores on the topic, nor queues
for an absent subscriber, nor writes to its persistence file. A discovery
announcement describes what is on the network *now*; retained, it goes on
describing a stream that was switched off, and the agent that would correct it need not be running.

**Stopping is the retraction.** The empty-retained-payload deletes this agent
used to publish are gone — there is nothing left to delete.

### The restate timer is load-bearing

mDNS resolution is an *event*, and a RAVENNA session that resolves once may not resolve again while its records stay valid. Publishing only on that event would put the picture on the bus once
and leave it invisible to every subscriber that connected afterwards. So the
full set is restated every **30s** from values already in hand, and the dedupe
that used to skip unchanged attributes is gone: skipping a write is free only
while the broker is holding the last copy of it.

| | |
|---|---|
| A fresh subscriber | has the full picture within 30s |
| A device that leaves | stops being restated; nothing to clear |
| While the agent is down | the tree is **empty** |

Presence (`Discovery/ravenna/{id}`) and the `config.ini` publication stay
retained: both belong to `apk-plugin-runner`, are shared with every plugin, and
presence is what says a device has *gone* rather than merely gone quiet.

`nothing_this_agent_publishes_is_kept` enforces this at the source — one publish
call in the crate, and it is `say`.

**No SDP is re-fetched to restate.** The cached values are republished; turning a bus policy into an RTSP DESCRIBE against every node on the bench every thirty seconds would be a worse bug than the one it fixes.

A session that goes (`ServiceRemoved`) drops out of the restate set at once;
its host's presence goes inactive only when the host's **last** session goes.

## Configuration

`SRC/config.ini`, `[ravenna]`: `enabled`, and the presence pair the runner
reads — `presence_protocols = ravenna`, `presence_inactive_after_s = 0`
(departures only). `service_type` and `domain` are not read by code. The broker
is `MQTT_HOST` / `MQTT_PORT` from the runner, which also publishes the retained
config at `APK.audio/System/Protocols/ravenna/config`, the `incoming/*` module
topics and the self test (`src/self_test.rs`). `SRC/gui/manifest.json` declares
the `ravenna` console family (tree `7_Device_Consoles`) and
`SRC/gui/ravenna/ravenna.json` is its panel.

## Build, run, test

```sh
# from PODS/POD.THICK/DISCOVERY.Engine/SRC
cargo test -p apkaudio-ravenna        # 12 tests (2026-10-30)

COMPOSE_PROFILES=discovery docker compose -f PODS/POD.THICK/PLUGINS/PLUGIN.RAVENNA/Docker/docker-compose.yml up -d --build
```

`apk-ravenna` takes no arguments — `--help` starts the agent against the broker.

## Philosophy

- **Refusal over fabrication.** An RTSP responder is not a RAVENNA device until
  its SDP says audio — "a guess dressed as a discovery" is refused
  (`src/lib.rs`, "Why the SDP fetch is not optional").
- **Read, never change.** A DESCRIBE is what any receiver does before it
  subscribes, and changes nothing on the device (`src/lib.rs`, "Scope").
- **Nothing is kept; stopping is the retraction.** QoS 0, retain clear,
  restated every 30 s from the cache (`say`, `RESTATE`,
  `nothing_this_agent_publishes_is_kept`).
- **A bus policy must not become network load.** The restate never re-fetches
  an SDP (`restate()`).
- **A device is gone when it is all gone.** One dropped stream does not take a
  live host out of the list (`ServiceRemoved` branch).
- **One parser, shared.** DANTE and SAP use this crate's `sdp` rather than a
  second copy (`PLUGIN.SAP/SRC/Cargo.toml`).

---

*© 2026 — Written & Maintained by Anthony P. Kuzub. MIT Licence.*

<!-- wiki:smoke -->
**Smoke test** — ✅ PASS 2/2 since 2026-10-30 12:50 EDT — [results](Documentation/%F0%9F%A7%A9Manual%20and%20Artifacts/SMOKE.md)
<!-- /wiki:smoke -->
