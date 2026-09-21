# APK:plugin:RAVENNA

RAVENNA / AES67 browse, one container.

| | |
|---|---|
| **Recipe** | `Docker/docker-compose.yml` — no Dockerfile: it runs a binary from the shared plugin image built by `APK:discovery` |
| **Source** | `SRC/` (this repository — https://github.com/LikeDotAudio/APK.plugin.discovery.RAVENNA.git) — bin `apk-ravenna`; built by the shared plugin image |
| **Profile** | `discovery` |
| **Talks to** | the broker only |

Was a thread in `workers/discovery.rs`. The crate stays a library to DANTE and SAP for its `sdp` module. Moved into its own container by PLAN-3316.01.

---

*© 2026 — Written & Maintained by Anthony P. Kuzub. MIT Licence.*
