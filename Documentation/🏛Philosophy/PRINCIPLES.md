# Principles PLUGIN.RAVENNA keeps

Each rule is pointed at the code that keeps it; where the code bends one, it says so. Ecosystem
rules live in the root: [Refusal over fabrication](../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Refusal%20over%20fabrication.md),
[Doctrine](../../../../../../Documentation/%F0%9F%8F%9BPhilosophy/Doctrine/readme.Doctrine.md).

← [Philosophy](INDEX.md) · [Repo README](../../README.md)

| Principle | Kept by |
|---|---|
| **Refusal over fabrication.** An RTSP responder is not a RAVENNA device until its own SDP says `m=audio`; port 554 alone is also every IP camera. | the `!stream.is_audio` guard in `Browse::hear`, [`SRC/src/agent.rs`](../../SRC/src/agent.rs); module doc "Why the SDP fetch is not optional"; test `video_and_silent_services_are_not_filed`. |
| **Read, never change.** One RTSP `DESCRIBE` per resolution — what any receiver does before subscribing — and nothing else: no RTP, no subscription, no connection management. | `describe`, `SRC/src/agent.rs`; module doc "Scope". |
| **Nothing is kept; stopping is the retraction.** Every stream topic is QoS 0, retain clear, through one helper; the cached values are restated every 30 s. | `apk_plugin_runner::unkept::say` / `restate` (the runner's one QoS 0 / not-retained pair); `restate_s` in `config.ini`, read by `Settings::from_ini`; test `nothing_this_agent_publishes_is_kept`. |
| **A bus policy must not become network load.** The restate republishes cached values; it never re-fetches an SDP. | `Browse::fields` → `unkept::restate`, `SRC/src/agent.rs`. |
| **A device is gone when it is all gone.** A removed session leaves the restate set at once; the host's presence goes inactive only with its last session. | the `Heard::Removed` arm of `Browse::hear`, `SRC/src/agent.rs`; test `a_host_goes_with_its_last_session`. |
| **One reader per wire format, shared.** SDP, SAP, PTPv2 and the RTSP `/by-name/` path are read here once; `PLUGIN.AES67`, `PLUGIN.SAP`, `PLUGIN.DANTE` and `PLUGIN.PTP` take them with `default-features = false` (no agent, no mDNS) instead of keeping copies, and re-run the SDP corpus against the rev they pin. | [`SRC/src/sdp.rs`](../../SRC/src/sdp.rs), [`sap.rs`](../../SRC/src/sap.rs), [`ptp.rs`](../../SRC/src/ptp.rs), [`rtsp.rs`](../../SRC/src/rtsp.rs); `corpus()` + [`SRC/vectors/sdp/`](../../SRC/vectors/sdp); feature `agent` in [`SRC/Cargo.toml`](../../SRC/Cargo.toml). |
| **A dual-homed node is one device.** Streams are grouped by hostname, and a host keeps every address its sessions came from, so the ST 2022-7 red/blue pair is not two devices. | `ravenna_sighting`, `SRC/src/agent.rs`. |

## Where the code bends a rule

* **Identity = serial/MAC.** The sighting is keyed on the mDNS **hostname**: mDNS and SDP carry no
  serial and no MAC, so for this protocol the hostname is the stated fallback identity — never an
  IP (an ST 2022-7 node has two). It survives a DHCP change and dual-homing but not a rename.
* **Stall watchdog.** `run_browse_agent` calls `apk_plugin_runner::tick()` every turn (at least every
  `restate_s`, 30 s), so a wedged loop is an exit after `APK_PLUGIN_STALL_SECS`.
* **Declared config is read config.** `config.ini` declares no `service_type`/`domain` and no
  `topic`: the service types are the wire constant `SERVICE_TYPES`, and every topic is built by the
  contract's `topics::protocols("ravenna", …)` in one function (`device_topic`).

## Kept by design

* **The compose file `extends` `../../../DISCOVERY.Engine/Docker/plugin-base.yml`, and the crate
  builds in the engine workspace through its `[patch]` tables and `SRC/members/` link.** That is
  the house plugin pattern (`.apk.scripts/plugin_standalone.py` docstring): one base service and
  one image for every plugin. Standalone, the crate still builds from this repository alone (SDK
  by tag).
* **The shared readers stay in this repository rather than a new one.** No new repository was
  wanted; a `default-features = false` dependency already gives consumers the readers without the
  agent's dependencies, and the engine's existing `[patch]` entry for this URL covers them.
