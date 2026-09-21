# APK:plugin:RAVENNA

RAVENNA / AES67 browse, one container.

| | |
|---|---|
| **Recipe** | `Docker/docker-compose.yml` — no Dockerfile: it runs a binary from the shared plugin image built by `APK:discovery` |
| **Source** | `SRC/` (this repository — https://github.com/LikeDotAudio/APK.plugin.discovery.RAVENNA.git) — bin `apk-ravenna`; built by the shared plugin image |
| **Profile** | `discovery` |
| **Talks to** | the broker only |

Was a thread in `workers/discovery.rs`. The crate stays a library to DANTE and SAP for its `sdp` module. Moved into its own container by PLAN-3316.01.


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

---

*© 2026 — Written & Maintained by Anthony P. Kuzub. MIT Licence.*
