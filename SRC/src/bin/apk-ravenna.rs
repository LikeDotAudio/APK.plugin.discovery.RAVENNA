// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `apk-ravenna` — RAVENNA / AES67 browse as its own container (PLAN-3316.01). The crate is still a LIBRARY to
//! DANTE and SAP for its `sdp` module; this binary is the one place it also runs.

//!
//! `panels = [ravenna]`: every audio stream presents its own panel, retained at
//! `APK.audio/System/Gui/Panels/ravenna-<host>-<stream>`.
//!
//! `manifest = "agent.toml"` (ROAD-3478): the manifest is compiled in, and the
//! runner serves `GET /status` / `GET /telemetry` on 127.0.0.1:8213 — `running`
//! until a stream resolves on mDNS or a device answers a DESCRIBE, `serving` after.

apk_plugin_runner::main!(token = "ravenna", lib = apkaudio_ravenna, run = [run_browse_agent], panels = [ravenna], manifest = "agent.toml");
