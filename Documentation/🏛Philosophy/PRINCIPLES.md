# Principles PLUGIN.RAVENNA keeps

Each rule is pointed at the code that keeps it; where the code bends one, it says so. Ecosystem
rules live in the root: [Refusal over fabrication](../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Refusal%20over%20fabrication.md),
[Doctrine](../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Doctrine.md).

← [Philosophy](INDEX.md) · [Repo README](../../README.md)

| Principle | Kept by |
|---|---|
| **Refusal over fabrication.** An RTSP responder is not a RAVENNA device until its own SDP says `m=audio`; port 554 alone is also every IP camera. | the `!stream.is_audio` guard, [`SRC/src/lib.rs:291`](../../SRC/src/lib.rs); module doc "Why the SDP fetch is not optional". |
| **Read, never change.** One RTSP `DESCRIBE` per resolution — what any receiver does before subscribing — and nothing else: no RTP, no subscription, no connection management. | `describe`, `SRC/src/lib.rs:145`; module doc "Scope". |
| **Nothing is kept; stopping is the retraction.** Every stream topic is QoS 0, retain clear, through one helper; the cached values are restated every 30 s. | `say`, `SRC/src/lib.rs:110`; `RESTATE` (`:103`); test `nothing_this_agent_publishes_is_kept`. |
| **A bus policy must not become network load.** The restate republishes cached values; it never re-fetches an SDP. | `restate`, `SRC/src/lib.rs:116`. |
| **A device is gone when it is all gone.** A removed session leaves the restate set at once; the host's presence goes inactive only with its last session. | the `ServiceRemoved` branch, `SRC/src/lib.rs:365` (`fields.retain`, `hosts.remove`). |
| **One parser, shared.** `PLUGIN.DANTE` and `PLUGIN.SAP` use this crate's `sdp` module rather than a copy. | [`SRC/src/sdp.rs`](../../SRC/src/sdp.rs) `parse`; `apkaudio-ravenna` in their `Cargo.toml`. |
| **A dual-homed node is one device.** Streams are grouped by hostname, and a host keeps every address its sessions came from, so the ST 2022-7 red/blue pair is not two devices. | `ravenna_sighting`, `SRC/src/lib.rs:496`. |

## Where the code bends a rule

* **Identity = serial/MAC.** The sighting is keyed on the mDNS **hostname** (no serial is given),
  which survives a DHCP change and dual-homing but not a rename of the node.
* **Stall watchdog.** `run_browse_agent` does not call `apk_plugin_runner::tick()`.
* **Declared config is read config.** `service_type` and `domain` in
  [`SRC/config.ini`](../../SRC/config.ini) are read by nothing; the service types are the wire
  constant `SERVICE_TYPES`.
