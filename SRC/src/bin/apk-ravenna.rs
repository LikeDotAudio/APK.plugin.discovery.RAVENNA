// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `apk-ravenna` — RAVENNA / AES67 browse as its own container (PLAN-3316.01). The crate is still a LIBRARY to
//! DANTE and SAP for its `sdp` module; this binary is the one place it also runs.

apk_plugin_runner::main!(token = "ravenna", lib = apkaudio_ravenna, run = [run_browse_agent]);
