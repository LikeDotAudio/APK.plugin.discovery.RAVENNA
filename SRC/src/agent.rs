// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The RAVENNA browse agent (feature `agent`): the two vendor adapters, the
//! browse state, and the loop that joins them to the bus.
//!
//! * **Adapters** — [`heard_from`] turns an `mdns_sd::ServiceEvent` into a
//!   [`Heard`]; [`Rtsp`] is the [`Describe`] that fetches an SDP over RTSP.
//!   Nothing else here names `mdns_sd` or a `TcpStream`.
//! * **[`Browse`]** — what is known, and what each [`Heard`] changes, as a list
//!   of [`Effect`]s. Pure: tests drive it with canned announcements and a
//!   canned describer, no network.
//! * **[`run_browse_agent`]** — the loop: mDNS in, effects out through the
//!   runner's `Session` and `unkept::say`, the restate timer.

use crate::{rtsp, sdp, self_test};
use apk_plugin_runner::session::Session;
use apk_plugin_runner::unkept::{restate, say};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use apkaudio_contracts::device_publish::Sighting;
use apkaudio_contracts::device_record::{
    DeviceRecordExtra, DeviceRecordExtraRavenna, DeviceRecordExtraRavennaStreamsItem, DeviceRecordStatus,
};
use apkaudio_contracts::presence;
use apkaudio_contracts::topics::{self, sanitize_segment};

/// Service types that lead to a RAVENNA node.
///
/// `_ravenna._tcp` is definitive when present; many devices only advertise the
/// generic `_rtsp._tcp`, which is why the SDP check exists.
const SERVICE_TYPES: [&str; 2] = ["_ravenna._tcp.local.", "_rtsp._tcp.local."];

/// Retained attribute keys per stream. Order here is the UI column order.
const STREAM_KEYS: [&str; 14] = [
    "stream",
    // The announcing host, as mDNS named it. The stream's own topic carries
    // it only as a sanitized path segment; the console's Host field reads this.
    "host",
    "format",
    "sample_rate",
    "channels",
    "destination",
    "rtp_port",
    "ptime_ms",
    "clock_domain",
    "refclk",
    "direction",
    "status",
    // `-1` while the session is announcing — see
    // `apkaudio_contracts::presence`.
    "last_online",
    "online_since",
];

/// What `config.ini`'s `[ravenna]` section tunes, read once at start (RP-14:
/// intervals and timeouts are config, not source).
///
/// * `restate` (`restate_s`) — how often the whole known picture is restated.
///   Nothing is retained, so this is not a refresh of a stored value — it is
///   the only reason a value is on the bus at all between mDNS events.
/// * `rtsp_timeout` (`rtsp_timeout_s`) — how long one RTSP DESCRIBE may take
///   to connect, write or read before the session is skipped as "no SDP".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub restate: Duration,
    pub rtsp_timeout: Duration,
}

impl Settings {
    /// Both keys are required, in whole seconds above zero; a missing or
    /// wrong one is NAMED rather than defaulted.
    pub fn from_ini(text: &str) -> Result<Settings, String> {
        let config = apk_plugin_runner::ini_to_json(text);
        let section = config
            .get(self_test::MODULE)
            .ok_or_else(|| format!("config.ini has no [{}] section", self_test::MODULE))?;
        let seconds = |key: &str| -> Result<Duration, String> {
            let raw = section
                .get(key)
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("config.ini declares no {key}"))?;
            let n: u64 = raw.parse().map_err(|_| format!("{key} = {raw} is not whole seconds"))?;
            if n == 0 {
                return Err(format!("{key} = 0 is not an interval"));
            }
            Ok(Duration::from_secs(n))
        };
        Ok(Settings { restate: seconds("restate_s")?, rtsp_timeout: seconds("rtsp_timeout_s")? })
    }
}

/// The topic every RAVENNA host, and every stream under it, lives below:
/// `…/ravenna/Device/{host}[/{stream}]`, from the contract's `protocols`
/// builder rather than spelled here (RP-07). The segments are
/// `sanitize_segment` output, so the contract's refusal is unreachable.
fn device_topic(path: &[&str]) -> String {
    let mut full = vec!["Device"];
    full.extend_from_slice(path);
    topics::protocols(self_test::MODULE, &full).expect("sanitized segments are always topic segments")
}

/// Fetch the SDP for one session via RTSP DESCRIBE.
///
/// Returns `None` when the device does not answer, or answers without SDP —
/// both of which mean "do not claim this is a RAVENNA stream".
pub fn describe(ip: &str, port: u16, session: &str, timeout: Duration) -> Option<String> {
    let url = rtsp::by_name_url(ip, port, session);
    let addr = format!("{ip}:{port}").to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;

    let request = format!(
        "DESCRIBE {url} RTSP/1.0\r\nCSeq: 1\r\nAccept: application/sdp\r\n\
         User-Agent: APK.audio-discovery\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    // Read until the socket goes quiet or a sane cap is hit. These records are a
    // few hundred bytes; anything larger is not an SDP we understand.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while buf.len() < 8192 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                // Header + body delivered; stop rather than wait for the timeout.
                if buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() > 120 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let text = String::from_utf8_lossy(&buf).to_string();
    // TRAFFIC, counted for the loopback telemetry (ROAD-3478): the device
    // ANSWERED the DESCRIBE — any RTSP reply, 200 or not. A refused connect or
    // a silent socket counts nothing.
    if text.starts_with("RTSP/") {
        apk_plugin_runner::loopback::packet_received();
    }
    if !text.starts_with("RTSP/1.0 200") {
        return None;
    }
    Some(text)
}

/// Fetches one session's SDP — the RTSP side of browsing, as a seam. The
/// agent uses [`Rtsp`]; tests use a canned map.
pub trait Describe {
    /// The SDP (or RTSP reply carrying it) for `session` at `ip:port`, or
    /// `None` when there is none — which means "do not claim this stream".
    fn describe(&self, ip: &str, port: u16, session: &str) -> Option<String>;
}

/// [`describe`] over a real TCP connection, with `config.ini`'s timeout.
pub struct Rtsp {
    pub timeout: Duration,
}

impl Describe for Rtsp {
    fn describe(&self, ip: &str, port: u16, session: &str) -> Option<String> {
        describe(ip, port, session, self.timeout)
    }
}

/// One resolved mDNS service, as the browse needs it — the mDNS adapter's
/// output, so the browse never sees an `mdns_sd` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// The announcing host as mDNS named it, trailing dot removed.
    pub host: String,
    /// The service's full name, e.g. `Desk preMO._rtsp._tcp.local.`.
    pub fullname: String,
    /// The RTSP port the service is on.
    pub port: u16,
    /// Every address the record carried.
    pub addresses: Vec<String>,
}

/// What mDNS said, in the browse's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Resolved(Announcement),
    /// A service left, by its full name.
    Removed(String),
}

/// The mDNS adapter: the two events the browse acts on, nothing else.
fn heard_from(event: ServiceEvent) -> Option<Heard> {
    match event {
        ServiceEvent::ServiceResolved(info) => Some(Heard::Resolved(Announcement {
            host: info.get_hostname().trim_end_matches('.').to_string(),
            fullname: info.get_fullname().to_string(),
            port: info.get_port(),
            addresses: info.get_addresses().iter().map(|a| a.to_string()).collect(),
        })),
        ServiceEvent::ServiceRemoved(_stype, fullname) => Some(Heard::Removed(fullname)),
        _ => None,
    }
}

/// `Desk preMO._rtsp._tcp.local.` → `Desk preMO`.
fn session_of(fullname: &str) -> String {
    fullname.split_once("._").map(|(s, _)| s.to_string()).unwrap_or_else(|| fullname.to_string())
}

/// What one [`Heard`] makes the agent do.
#[derive(Debug)]
pub(crate) enum Effect {
    /// Say `value` on `topic`, unkept.
    Say(String, String),
    /// A host was sighted; its detail topic.
    Seen(Sighting, String),
    /// A host's last session left: its device id goes inactive.
    Gone(String),
}

/// Everything the browse knows, and the rules for changing it.
pub(crate) struct Browse {
    /// Session -> host, so a vanished announcement clears the right topics.
    published: HashMap<String, String>,
    /// What was last said per topic. mDNS re-resolves a live session every
    /// few seconds; both presence fields are constants now, so a stream whose
    /// SDP has not changed is not news (it is still said — see `hear`).
    fields: HashMap<String, String>,
    /// When each session arrived.
    arrived: presence::Presence,
    /// The presence side (PLAN-3318.01), and it is per HOST rather than per
    /// stream. A RAVENNA session is a FLOW — the SDP is a description of audio
    /// moving, not of a box. The thing a controller should see in its device
    /// list is the interface announcing those sessions, and one interface
    /// announces many.
    hosts: HashMap<String, RavennaHost>,
}

impl Browse {
    pub(crate) fn new() -> Self {
        Browse { published: HashMap::new(), fields: HashMap::new(), arrived: presence::Presence::new(), hosts: HashMap::new() }
    }

    /// The whole known picture, for the restate timer. The cache is the
    /// announcement, not a copy of one: exactly what the last resolutions produced.
    pub(crate) fn fields(&self) -> &HashMap<String, String> {
        &self.fields
    }

    /// Every known host as a sighting, for the restate timer (ROAD-3437 step
    /// 3). The engine judges a host alive only while its `…/verified` proof
    /// is younger than 3 × the verify period, and the runner restates the
    /// proof only on a sighting — but mDNS re-resolves on its own schedule,
    /// not ours. An unchanged host costs presence a map lookup.
    pub(crate) fn sightings(&self) -> Vec<Effect> {
        let mut hosts: Vec<(&String, &RavennaHost)> = self.hosts.iter().collect();
        hosts.sort_by(|a, b| a.0.cmp(b.0));
        hosts.into_iter().map(|(seg, host)| Effect::Seen(ravenna_sighting(host), device_topic(&[seg]))).collect()
    }

    /// Fold one mDNS event in; the effects are what to say and sight.
    pub(crate) fn hear(&mut self, heard: Heard, describer: &dyn Describe, now_secs: u64) -> Vec<Effect> {
        let mut effects = Vec::new();
        match heard {
            Heard::Resolved(info) => {
                let Some(ip) = info.addresses.iter().find(|a| a.contains('.')).cloned() else {
                    return effects; // IPv6-only announcement; RAVENNA here is v4.
                };
                let host = info.host.clone();
                let session = session_of(&info.fullname);

                // The SDP is what proves this is audio. No SDP, no claim.
                let Some(raw) = describer.describe(&ip, info.port, &session) else {
                    println!("   ⏭️  [RAVENNA] {session} @ {ip} did not return SDP — skipping");
                    return effects;
                };
                let stream = sdp::parse(&raw);
                if !stream.is_audio {
                    println!("   ⏭️  [RAVENNA] {session} @ {ip} is not an audio stream — skipping");
                    return effects;
                }

                let host_seg = sanitize_segment(&host);
                let stream_seg = sanitize_segment(&session);
                let prefix = device_topic(&[&host_seg, &stream_seg]);

                let values = [
                    session.clone(),
                    host.clone(),
                    stream.format_summary(),
                    stream.sample_rate.to_string(),
                    stream.channels.to_string(),
                    stream.destination.clone(),
                    stream.rtp_port.to_string(),
                    stream.ptime_ms.clone(),
                    stream.clock_domain.clone(),
                    stream.refclk.clone(),
                    stream.direction.clone(),
                    "identified".to_string(),
                    presence::ONLINE_NOW.to_string(),
                    self.arrived.since(&prefix, now_secs).to_string(),
                ];

                // EVERY field, EVERY resolution — and again on the restate
                // timer. The dedupe that used to live here was free only while
                // the broker held the last copy; unkept, a skipped write is a
                // value no later subscriber can ever see. `wrote` counts what
                // is NEWS, which is a question about the log line only.
                let mut wrote = 0usize;
                for (key, value) in STREAM_KEYS.iter().zip(values) {
                    let topic = format!("{prefix}/{key}");
                    if self.fields.get(&topic).is_none_or(|prev| *prev != value) {
                        wrote += 1;
                    }
                    self.fields.insert(topic.clone(), value.clone());
                    effects.push(Effect::Say(topic, value));
                }
                // Fold this session into what is known about its host, and
                // sight the host as one document. See `hosts`.
                let entry = self.hosts.entry(host_seg.clone()).or_insert_with(|| RavennaHost {
                    hostname: host.clone(),
                    addresses: BTreeSet::new(),
                    streams: BTreeMap::new(),
                });
                entry.addresses.insert(ip.clone());
                // The SAME values the STREAM_KEYS leaves above carry — the
                // engine builds its stream rows from the host's record.
                let text = |v: &str| (!v.trim().is_empty()).then(|| v.to_string());
                entry.streams.insert(
                    stream_seg.clone(),
                    DeviceRecordExtraRavennaStreamsItem {
                        session: session.clone(),
                        format: text(&stream.format_summary()),
                        sample_rate: Some(i64::from(stream.sample_rate)),
                        channels: Some(i64::from(stream.channels)),
                        destination: text(&stream.destination),
                        rtp_port: Some(i64::from(stream.rtp_port)),
                        ptime_ms: text(&stream.ptime_ms),
                        clock_domain: text(&stream.clock_domain),
                        refclk: text(&stream.refclk),
                        direction: text(&stream.direction),
                    },
                );
                effects.push(Effect::Seen(ravenna_sighting(entry), device_topic(&[&host_seg])));

                // Only when it is news — a session re-announcing is not one.
                if wrote > 0 {
                    println!(
                        "   ✅ [RAVENNA] {host} / {session} — {} -> {}:{} ({})",
                        stream.format_summary(),
                        stream.destination,
                        stream.rtp_port,
                        stream.clock_domain
                    );
                }
                self.published.insert(stream_seg, host_seg);
            }
            Heard::Removed(fullname) => {
                let session = session_of(&fullname);
                let stream_seg = sanitize_segment(&session);
                // Only clear what we actually published — a removal for a service
                // that never passed the audio check is not ours to act on.
                let Some(host_seg) = self.published.remove(&stream_seg) else { return effects };
                println!("   👋 [RAVENNA] removed {session}");
                // The host goes inactive only when its LAST session goes. An
                // interface that drops one of four streams is still on the
                // network, and saying it gone here would take a live device
                // out of a controller's list because one flow stopped.
                if let Some(entry) = self.hosts.get_mut(&host_seg) {
                    entry.streams.remove(&stream_seg);
                    if entry.streams.is_empty() {
                        if let Some(gone) = self.hosts.remove(&host_seg) {
                            effects.push(Effect::Gone(ravenna_sighting(&gone).device_id()));
                        }
                    } else {
                        // Still here with fewer streams: re-sight it, so its
                        // record drops the stream that left.
                        effects.push(Effect::Seen(ravenna_sighting(entry), device_topic(&[&host_seg])));
                    }
                }
                let prefix = device_topic(&[&host_seg, &stream_seg]);
                // No topics to clear: unkept, a stream leaves the bus by
                // dropping out of `fields` and so out of the next restate.
                // Forgetting both also means a session that comes back is
                // announced in full and starts a NEW spell.
                self.fields.retain(|topic, _| !topic.starts_with(&prefix));
                self.arrived.forget(&prefix);
            }
        }
        effects
    }
}

/// Blocking browse loop — run on a dedicated thread.
pub fn run_browse_agent(mqtt_host: &str, mqtt_port: u16) {
    let settings = match Settings::from_ini(self_test::CONFIG_INI) {
        Ok(s) => s,
        Err(why) => {
            // The compiled-in file is wrong: a build defect, said once, loudly.
            eprintln!("🛑 [RAVENNA] {why}");
            apk_plugin_runner::loopback::fatal(why);
            return;
        }
    };
    // One session constructor for every plugin (credentials, unique client
    // id, broker backoff on the drain thread): apk_plugin_runner::session.
    let mqtt_client = Session::new(self_test::MODULE, "browse").to(mqtt_host, mqtt_port).capacity(32).spawn_drained();

    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("❌ [RAVENNA] could not start mDNS daemon: {e}");
            apk_plugin_runner::loopback::fatal(format!("could not start mDNS daemon: {e}"));
            return;
        }
    };

    let (tx, rx) = std::sync::mpsc::channel::<Heard>();
    for stype in SERVICE_TYPES {
        match mdns.browse(stype) {
            Ok(receiver) => {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(ev) = receiver.recv() {
                        let Some(heard) = heard_from(ev) else { continue };
                        // TRAFFIC (ROAD-3478): a service RESOLVED from mDNS
                        // responses. A removal is not counted — mdns-sd raises
                        // it for a goodbye and a lapsed TTL alike.
                        if matches!(heard, Heard::Resolved(_)) {
                            apk_plugin_runner::loopback::packet_received();
                        }
                        if tx.send(heard).is_err() {
                            break;
                        }
                    }
                });
            }
            Err(e) => eprintln!("   ⚠️  [RAVENNA] cannot browse {stype}: {e}"),
        }
    }
    drop(tx);

    println!("🚀 [RAVENNA] browsing {:?} (discovery + SDP description only)", SERVICE_TYPES);

    let rtsp = Rtsp { timeout: settings.rtsp_timeout };
    let mut browse = Browse::new();

    // The restate clock. mDNS hands this loop an event when a record is
    // learned, not on a schedule, so without this a quiet bench would publish
    // nothing between resolutions — and unkept, publishing nothing means being
    // invisible rather than being unchanged.
    let mut last_restate = std::time::Instant::now();

    loop {
        // THE STALL WATCHDOG (LOOSE-plugins-playstation-printers-ptp). The loop
        // turns at least every restate_s even on a silent network, so a loop that
        // wedges without returning is an exit the container restarts
        // (APK_PLUGIN_STALL_SECS).
        apk_plugin_runner::tick();
        let heard = match rx.recv_timeout(settings.restate) {
            Ok(heard) => Some(heard),
            // Idle: no mDNS traffic, which is the normal state of a settled
            // bench and exactly when the restate matters most.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            // Every browse thread has gone; there is nothing left to hear.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Checked on the event path too, so a bench chatty enough to keep
        // `recv_timeout` from ever expiring still restates on schedule.
        let mut effects = Vec::new();
        if last_restate.elapsed() >= settings.restate {
            last_restate = std::time::Instant::now();
            restate(&mqtt_client, browse.fields());
            // And every host re-sighted, so its `…/verified` proof stays fresh.
            effects.extend(browse.sightings());
        }

        if let Some(heard) = heard {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            effects.extend(browse.hear(heard, &rtsp, now_secs));
        }
        for effect in effects {
            match effect {
                Effect::Say(topic, value) => say(&mqtt_client, topic, value.into_bytes()),
                Effect::Seen(sighting, detail) => apk_plugin_runner::presence::seen(&sighting, Some(&detail)),
                Effect::Gone(device_id) => apk_plugin_runner::presence::gone(&device_id),
            }
        }
    }
    // Every browse thread has gone: nothing is left to hear.
    eprintln!("🛑 [RAVENNA] every mDNS browse ended");
    apk_plugin_runner::loopback::fatal("every mDNS browse ended");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest the binary compiles in parses, declares the loopback port
    /// and lists the keys of config.ini's [ravenna] it reads (ROAD-3478).
    /// ONE KEY IS UNREAD, honestly: `enabled` — neither this crate nor the
    /// runner reads it.
    #[test]
    fn the_compiled_in_manifest_parses_and_reads_every_config_key() {
        let manifest = apkaudio_contracts::embedded_manifest!().expect("agent.toml parses");
        assert_eq!(manifest.name, "ravenna");
        assert_eq!(manifest.telemetry_port, 8213);
        let config = apk_plugin_runner::ini_to_json(include_str!("../config.ini"));
        let keys: Vec<&str> = config["ravenna"].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(manifest.unread_config_keys(keys), Some(1), "[ravenna] keys nothing reads: only `enabled`");
    }

    /// The unkept policy, enforced rather than merely documented.
    ///
    /// Every publish in this agent goes through `say`, the single place the QoS
    /// and retain flags are chosen. Most other agents in this tree publish
    /// retained at QoS 1, so reaching for the client directly is the natural
    /// thing for a later edit to do — and one such call would put a RAVENNA stream announcement
    /// back into the broker's store, where it keeps answering after the thing
    /// it describes has left the network. Checked at the source rather than
    /// left to review.
    ///
    /// The needles are assembled at runtime: spelled out literally they would
    /// appear in this test and match themselves.
    #[test]
    fn nothing_this_agent_publishes_is_kept() {
        let source = include_str!("agent.rs");
        let call = format!(".{}(", "publish");
        assert_eq!(
            source.matches(&call).count(),
            0,
            "every publish must go through apk_plugin_runner::unkept::say; found a direct client call"
        );
        assert!(source.contains(&format!("unkept::{{restate, {}}}", "say")), "say/restate are the runner's unkept pair");

        for kept in [format!("At{}Once", "Least"), format!("Exactly{}", "Once")] {
            assert!(
                !source.contains(&kept),
                "{kept} would queue this agent's messages for absent subscribers"
            );
        }
    }

    /// A canned RTSP: SDP per session name.
    struct Canned(HashMap<&'static str, &'static str>);
    impl Describe for Canned {
        fn describe(&self, _ip: &str, _port: u16, session: &str) -> Option<String> {
            self.0.get(session).map(|s| s.to_string())
        }
    }

    fn canned() -> Canned {
        let corpus = sdp::corpus();
        let desk = corpus.iter().find(|c| c.name == "merging-desk-premo.rtsp").unwrap().sdp;
        let camera = corpus.iter().find(|c| c.name == "ip-camera-video.rtsp").unwrap().sdp;
        Canned([("Desk preMO", desk), ("Monitor", desk), ("Front Door", camera)].into_iter().collect())
    }

    fn resolved(session: &str, ip: &str) -> Heard {
        Heard::Resolved(Announcement {
            host: "desk.local".into(),
            fullname: format!("{session}._rtsp._tcp.local."),
            port: 554,
            addresses: vec!["fe80::1".into(), ip.into()],
        })
    }

    fn says(effects: &[Effect]) -> HashMap<String, String> {
        effects.iter().filter_map(|e| match e { Effect::Say(t, v) => Some((t.clone(), v.clone())), _ => None }).collect()
    }

    /// The whole browse, driven with canned announcements: an audio session
    /// is filed with every key and sights its host; the restate holds it.
    #[test]
    fn a_canned_audio_announcement_is_filed_under_its_host() {
        let mut b = Browse::new();
        let effects = b.hear(resolved("Desk preMO", "44.44.44.166"), &canned(), 1_000);
        let said = says(&effects);
        let prefix = "APK.audio/System/Protocols/ravenna/Device/desk_local/Desk_preMO";
        assert_eq!(said.len(), STREAM_KEYS.len());
        assert_eq!(said[&format!("{prefix}/format")], "L24 48000Hz 2ch");
        assert_eq!(said[&format!("{prefix}/destination")], "239.1.44.117");
        assert_eq!(said[&format!("{prefix}/host")], "desk.local");
        assert_eq!(said[&format!("{prefix}/online_since")], "1000");
        assert!(matches!(effects.last(), Some(Effect::Seen(_, d)) if d == "APK.audio/System/Protocols/ravenna/Device/desk_local"));
        assert_eq!(b.fields(), &said, "the restate says exactly what was heard");
    }

    /// Port 554 proves nothing: a camera's video SDP, or no SDP, files nothing.
    #[test]
    fn video_and_silent_services_are_not_filed() {
        let mut b = Browse::new();
        assert!(b.hear(resolved("Front Door", "10.0.0.2"), &canned(), 1).is_empty());
        assert!(b.hear(resolved("Unknown", "10.0.0.3"), &canned(), 1).is_empty());
        assert!(b.hear(Heard::Removed("Front Door._rtsp._tcp.local.".into()), &canned(), 2).is_empty());
        assert!(b.fields().is_empty());
    }

    /// A host goes only when its LAST session goes; a removed stream drops out
    /// of the restate.
    #[test]
    fn a_host_goes_with_its_last_session() {
        let mut b = Browse::new();
        b.hear(resolved("Desk preMO", "44.44.44.166"), &canned(), 1);
        b.hear(resolved("Monitor", "44.44.44.166"), &canned(), 1);
        let first = b.hear(Heard::Removed("Desk preMO._rtsp._tcp.local.".into()), &canned(), 2);
        assert!(!first.iter().any(|e| matches!(e, Effect::Gone(_))), "one stream left; the host is still here");
        assert!(b.fields().keys().all(|t| t.contains("/Monitor/")));
        let last = b.hear(Heard::Removed("Monitor._rtsp._tcp.local.".into()), &canned(), 3);
        assert!(matches!(last.as_slice(), [Effect::Gone(id)] if id.contains("desk.local")));
        assert!(b.fields().is_empty());
    }

    fn streams_of(effect: &Effect) -> Vec<DeviceRecordExtraRavennaStreamsItem> {
        match effect {
            Effect::Seen(s, _) => s.extra.as_ref().and_then(|e| e.ravenna.clone()).expect("extra.ravenna").streams,
            other => panic!("not a sighting: {other:?}"),
        }
    }

    /// ROAD-3437 step 3: the host's record carries every stream it announces,
    /// with the values its STREAM_KEYS leaves carry, sorted by session.
    #[test]
    fn a_host_with_two_streams_carries_both_in_its_extra() {
        let mut b = Browse::new();
        b.hear(resolved("Monitor", "44.44.44.166"), &canned(), 1);
        let effects = b.hear(resolved("Desk preMO", "44.44.44.166"), &canned(), 1);
        let streams = streams_of(effects.last().unwrap());
        assert_eq!(streams.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(), ["Desk preMO", "Monitor"]);
        let said = says(&effects);
        let prefix = "APK.audio/System/Protocols/ravenna/Device/desk_local/Desk_preMO";
        let desk = &streams[0];
        assert_eq!(desk.format.as_deref(), Some(said[&format!("{prefix}/format")].as_str()));
        assert_eq!(desk.destination.as_deref(), Some("239.1.44.117"));
        assert_eq!(desk.sample_rate.map(|n| n.to_string()), Some(said[&format!("{prefix}/sample_rate")].clone()));
        assert_eq!(desk.channels.map(|n| n.to_string()), Some(said[&format!("{prefix}/channels")].clone()));
        assert_eq!(desk.rtp_port.map(|n| n.to_string()), Some(said[&format!("{prefix}/rtp_port")].clone()));
        for (key, value) in [("ptime_ms", &desk.ptime_ms), ("clock_domain", &desk.clock_domain), ("refclk", &desk.refclk), ("direction", &desk.direction)] {
            let leaf = &said[&format!("{prefix}/{key}")];
            assert_eq!(value.clone().unwrap_or_default(), *leaf, "{key}");
            assert!(value.as_deref() != Some(""), "{key}: empty strings are None");
        }
        // The restate re-sights the host with the same record.
        let again = b.sightings();
        assert_eq!(again.len(), 1);
        assert_eq!(streams_of(&again[0]), streams);
    }

    /// Removing one of two streams re-sights the host with the one left.
    #[test]
    fn removing_a_stream_re_sights_the_host_without_it() {
        let mut b = Browse::new();
        b.hear(resolved("Desk preMO", "44.44.44.166"), &canned(), 1);
        b.hear(resolved("Monitor", "44.44.44.166"), &canned(), 1);
        let effects = b.hear(Heard::Removed("Desk preMO._rtsp._tcp.local.".into()), &canned(), 2);
        assert_eq!(effects.len(), 1, "{effects:?}");
        let streams = streams_of(&effects[0]);
        assert_eq!(streams.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(), ["Monitor"]);
        assert!(matches!(&effects[0], Effect::Seen(_, d) if d == "APK.audio/System/Protocols/ravenna/Device/desk_local"));
    }

    #[test]
    fn ipv6_only_announcements_are_skipped() {
        let mut b = Browse::new();
        let v6 = Heard::Resolved(Announcement { host: "h".into(), fullname: "Desk preMO._rtsp._tcp.local.".into(), port: 554, addresses: vec!["fe80::1".into()] });
        assert!(b.hear(v6, &canned(), 1).is_empty());
    }

    /// RP-07: the host and stream topics are the contract's `protocols` root.
    #[test]
    fn device_topics_are_the_contract_protocols_root() {
        assert_eq!(device_topic(&["desk"]), "APK.audio/System/Protocols/ravenna/Device/desk");
        assert_eq!(
            device_topic(&["desk", "Desk_preMO"]),
            topics::protocols("ravenna", &["Device", "desk", "Desk_preMO"]).unwrap()
        );
    }

    /// RP-14: the restate interval and the RTSP timeout are config.
    #[test]
    fn intervals_come_from_config_ini() {
        let s = Settings::from_ini(self_test::CONFIG_INI).expect("config.ini is a complete [ravenna]");
        assert_eq!(s.restate, Duration::from_secs(30));
        assert_eq!(s.rtsp_timeout, Duration::from_secs(4));
        let ok = "[ravenna]\nrestate_s = 30\nrtsp_timeout_s = 4\n";
        assert!(Settings::from_ini(&ok.replace("rtsp_timeout_s = 4\n", "")).unwrap_err().contains("rtsp_timeout_s"));
        assert!(Settings::from_ini(&ok.replace("restate_s = 30", "restate_s = soon")).unwrap_err().contains("restate_s"));
        assert!(Settings::from_ini(&ok.replace("restate_s = 30", "restate_s = 0")).is_err());
    }

    #[test]
    fn a_multi_homed_host_says_the_same_notes_whichever_address_was_heard_last() {
        let host = |order: &[&str]| RavennaHost {
            hostname: "APK-Shop.local".into(),
            addresses: order.iter().map(|a| a.to_string()).collect(),
            streams: BTreeMap::new(),
        };
        let one = ravenna_sighting(&host(&["172.22.0.1", "44.44.44.168", "172.19.0.1"]));
        let two = ravenna_sighting(&host(&["44.44.44.168", "172.19.0.1", "172.22.0.1"]));
        assert_eq!(format!("{one:?}"), format!("{two:?}"));
    }

    #[test]
    fn sanitize_segment_neutralises_topic_metacharacters() {
        assert_eq!(sanitize_segment("Desk preMO"), "Desk_preMO");
        assert_eq!(sanitize_segment("a/b+c#d"), "a_b_c_d");
        assert_eq!(sanitize_segment("host.local."), "host_local");
    }
}

/// One RAVENNA interface, as the sessions it announces add up to it.
#[derive(Clone)]
struct RavennaHost {
    hostname: String,
    /// Every address its sessions were announced from. A set, not the last
    /// one: a dual-homed (or, on the bench, docker-bridged) interface announces
    /// on each, and "the last one heard" made the identity's `notes` flip
    /// between them on every sweep — measured 2026-10-25, the NetBox sync
    /// PATCHed `ravenna:APK-Shop.local` run after run for it.
    addresses: BTreeSet<String>,
    /// Sanitised session name → the stream's details, for every session this
    /// host currently announces. The host is retracted when this empties — see
    /// the removal arm. The details are the record's `extra.ravenna.streams`
    /// (ROAD-3437 step 3): the engine shows one row per stream from them.
    streams: BTreeMap<String, DeviceRecordExtraRavennaStreamsItem>,
}

/// The interface behind a set of sessions, as a presence sighting.
///
/// IDENTITY IS THE HOSTNAME, not the address: RAVENNA interfaces are commonly
/// dual-homed for the ST 2022-7 red/blue pair, so keying on an address would
/// make one interface into two devices — and the SDP `destination` is a
/// multicast group, which is not the device at all. The class is "Audio
/// Interface", which `element.rs::KEYWORDS` maps to source+destination: a
/// RAVENNA node both sends and receives, and the per-stream `direction` field
/// above is where the per-flow answer already lives.
///
/// `extra.ravenna.streams` is every stream, sorted by session name, holding
/// only values that change when the SDP does — never a per-sweep stamp, which
/// would rewrite the record on every resolution.
fn ravenna_sighting(host: &RavennaHost) -> Sighting {
    let mut streams: Vec<DeviceRecordExtraRavennaStreamsItem> = host.streams.values().cloned().collect();
    streams.sort_by(|a, b| a.session.cmp(&b.session));
    Sighting::new("ravenna", &host.hostname)
        .class("Audio Interface")
        .made_by("-", &host.hostname)
        .notes(&format!(
            "{} session(s) at {}",
            host.streams.len(),
            host.addresses.iter().cloned().collect::<Vec<_>>().join(", ")
        ))
        .status(DeviceRecordStatus::Identified)
        .extra(DeviceRecordExtra { ravenna: Some(DeviceRecordExtraRavenna { streams }), ..Default::default() })
}

