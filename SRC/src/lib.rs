// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! `apkaudio-ravenna` — RAVENNA / AES67 audio stream discovery, and the one
//! home of the AoIP wire readers every APK.audio plugin shares.
//!
//! # Two things in one crate, split by a feature
//!
//! * **The shared AoIP readers** — always built, no dependency beyond `std`
//!   (`serde` derives behind the `serde` feature):
//!   [`sdp`] (RFC 4566, discovery reader + strict reader/writer, fixture
//!   corpus), [`sap`] (RFC 2974 header), [`ptp`] (IEEE 1588-2008 / 802.1AS
//!   messages). PLUGIN.SAP, PLUGIN.DANTE, PLUGIN.AES67 and PLUGIN.PTP depend on
//!   this crate with `default-features = false` and get only these.
//! * **The RAVENNA browse agent** — feature `agent` (default): mDNS, RTSP,
//!   the bus. [`run_browse_agent`] and [`self_test`] are behind it, and so are
//!   `mdns-sd`, `rumqttc`, the runner and the contracts.
//!
//! A RAVENNA node announces itself over mDNS and describes its audio streams
//! with SDP, fetched over RTSP. This agent follows that chain end to end:
//!
//! 1. **mDNS** — browse `_ravenna._tcp` and `_rtsp._tcp` for stream
//!    announcements (`SERVICE_TYPES`; `_http._tcp` is not browsed).
//! 2. **RTSP DESCRIBE** — ask each announced session for its SDP record at
//!    `rtsp://<ip>:554/by-name/<session>`.
//! 3. **SDP** — parse out what an engineer needs: encoding, sample rate,
//!    channel count, multicast destination, packet time, and the PTP clock
//!    domain the stream is locked to.
//!
//! # Why the SDP fetch is not optional
//!
//! Port 554 proves nothing on its own — IP cameras answer RTSP too. The SDP is
//! what distinguishes an audio node from a doorbell: an `m=audio` line with an
//! `L24/48000/2`-style rtpmap. Publishing every RTSP responder as a "RAVENNA
//! device" would be a guess dressed as a discovery, so a service is only filed
//! here once its own SDP says it carries audio.
//!
//! # Grouping
//!
//! One physical node commonly announces several streams — a desk might publish
//! `Digital inputs 1-2` and a monitor feed from the same host. Streams are
//! therefore grouped by **hostname**, so the UI shows one device with N streams
//! rather than N unrelated rows.
//!
//! # Scope
//!
//! Discovery and description only. No RTP receive, no stream subscription, no
//! connection management. Reading an SDP is exactly what any receiver does
//! before deciding whether to subscribe, and it changes nothing on the device.
//!
//! # Nothing this agent publishes is kept
//!
//! Every stream topic goes out at **QoS 0 with the retain flag clear** — the
//! one combination MQTT offers that the broker neither stores on the topic,
//! nor queues for an absent subscriber, nor writes to its persistence file. A
//! discovery announcement describes what is on the network *now*; retained, it
//! goes on describing a stream that was switched off, and the agent that would
//! correct it is not necessarily running.
//!
//! Unkept, an announcement retracts itself by not being restated — which is
//! why the restate (`restate_s`, [`Settings`]) exists and is load-bearing rather than decorative. mDNS
//! resolution is an *event*, and a RAVENNA session that resolves once may not
//! resolve again for as long as its records are valid; publishing only on that
//! event would put each stream on the bus once and leave it invisible to every
//! subscriber that connected afterwards. So the full picture is restated on a
//! timer from values already in hand — **no SDP is re-fetched to do it**, since
//! that would turn a bus policy into RTSP traffic against every node on the
//! bench every thirty seconds.
//!
//! Presence (`Discovery/ravenna/{host}`) is the exception and stays retained:
//! it belongs to `apk_plugin_runner`, is shared with every plugin, and is what
//! says a host has GONE rather than merely gone quiet.

pub mod ptp;
pub mod rtsp;
pub mod sap;
pub mod sdp;

#[cfg(feature = "agent")]
mod agent;
#[cfg(feature = "agent")]
pub use agent::{Announcement, Describe, Heard, Rtsp, Settings, describe, run_browse_agent};

/// This crate's own self test — what it can prove about itself with nothing
/// plugged in. Reached from the bus through `outgoing/selftest_run`; see
/// `apkaudio_contracts::self_test` for the two inputs and the withdrawal the
/// first of them performs. PLAN-459.01.
#[cfg(feature = "agent")]
pub mod self_test;
