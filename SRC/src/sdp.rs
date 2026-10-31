// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! SDP (RFC 4566) for RAVENNA, AES67 and SMPTE ST 2110-30 — the one reader
//! (and writer) every APK.audio plugin uses.
//!
//! Two readers over one line grammar, because the plugins ask two different
//! questions of the same text:
//!
//! * [`parse`] → [`StreamInfo`] — **discovery**: what does this announcement
//!   say, field by field, for a table cell? Lenient: unknown or absent fields
//!   stay empty rather than guessed, RTSP response headers before the body are
//!   tolerated. PLUGIN.RAVENNA, PLUGIN.SAP and PLUGIN.DANTE file streams from it.
//! * [`parse_stream`] → [`StreamDesc`] — **subscription**: can a receiver play
//!   this, and how? Strict: the first audio media section only, errors that
//!   name the line that was missing or wrong. [`generate`] is its inverse (the
//!   SDP a sender announces). PLUGIN.AES67 sends and receives with them.
//!
//! Both walk [`lines`] and read `c=` / `a=rtpmap:` through the same helpers, so
//! a fix to how a line is read reaches every plugin at once.
//!
//! What ST 2110-30 adds on top of AES67 is small and all of it is here: the
//! `channel-order` fmtp parameter (ST 2110-30 §6.2.2), the conformance level
//! that bounds channels × ptime (§6.2.1, levels A/AX/B/BX/C/CX), and the rule —
//! shared with AES67 §8 but optional in practice there — that `ts-refclk` and
//! `mediaclk:direct=` are present.
//!
//! A real record from the bench (Merging/Neumann hardware):
//!
//! ```text
//! v=0
//! o=- 5 0 IN IP4 44.44.44.166
//! s=Desk preMO
//! t=0 0
//! a=clock-domain:PTPv2 0
//! m=audio 5004 RTP/AVP 97
//! c=IN IP4 239.1.44.117/1
//! a=rtpmap:97 L24/48000/2
//! a=recvonly
//! a=ptime:1
//! a=ts-refclk:ptp=IEEE1588-2008:00-07-F5-FF-FE-00-54-72:0
//! a=mediaclk:direct=0
//! ```
//!
//! The fixture corpus every consumer can run is [`corpus`]
//! (`SRC/vectors/sdp/`).

use std::net::Ipv4Addr;

/// Every `<type>=<value>` line of an SDP (or of an RTSP reply carrying one),
/// trimmed, CR stripped; lines without `=` are skipped.
pub fn lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|raw| raw.trim_end_matches('\r').trim().split_once('='))
}

/// `c=IN IP4 239.1.44.117/1` → the address text and the TTL, if any.
fn connection_fields(value: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut f = value.split_whitespace();
    let _net = f.next()?;
    let family = f.next()?;
    let mut addr = f.next()?.split('/');
    Some((family, addr.next().unwrap_or(""), addr.next()))
}

/// `a=rtpmap:97 L24/48000/2` (the text after `rtpmap:`) → payload type,
/// encoding name, rate, channels. Mono streams omit the channel count, which
/// is 1; an absent or unreadable rate is `None`.
fn rtpmap_fields(arg: &str) -> Option<(&str, &str, Option<u32>, u32)> {
    let (pt, spec) = arg.trim().split_once(' ')?;
    let mut f = spec.trim().split('/');
    let enc = f.next().unwrap_or("");
    let rate = f.next().and_then(|v| v.parse().ok());
    let channels = f.next().and_then(|v| v.parse().ok()).unwrap_or(1);
    Some((pt.trim(), enc, rate, channels))
}

/// What a RAVENNA stream announcement tells us.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StreamInfo {
    /// `s=` — the session name, which is also the RTSP `/by-name/` key.
    pub session: String,
    /// `a=rtpmap:` encoding, e.g. `L24`.
    pub codec: String,
    /// Sample rate in Hz, e.g. 48000.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u32,
    /// `c=` destination — a multicast group for a live RAVENNA stream.
    pub destination: String,
    /// `m=audio <port>` — the RTP port.
    pub rtp_port: u16,
    /// `a=ptime:` in milliseconds. RAVENNA's low-latency profiles use 0.125–1ms.
    pub ptime_ms: String,
    /// `a=clock-domain:` — e.g. `PTPv2 0`.
    pub clock_domain: String,
    /// `a=ts-refclk:` — identifies the PTP grandmaster the stream is locked to.
    pub refclk: String,
    /// `a=recvonly` / `a=sendonly` / `a=sendrecv`.
    pub direction: String,
    /// True when an `m=audio` line is present.
    ///
    /// This is the honest discriminator between a RAVENNA node and any other
    /// device answering RTSP on 554 — an IP camera will describe `m=video`.
    /// Presence on port 554 alone proves nothing.
    pub is_audio: bool,
}

impl StreamInfo {
    /// Human summary for a table cell, e.g. `L24 48000Hz 2ch`.
    pub fn format_summary(&self) -> String {
        if !self.is_audio {
            return "not an audio stream".to_string();
        }
        let mut parts = Vec::new();
        if !self.codec.is_empty() {
            parts.push(self.codec.clone());
        }
        if self.sample_rate > 0 {
            parts.push(format!("{}Hz", self.sample_rate));
        }
        if self.channels > 0 {
            parts.push(format!("{}ch", self.channels));
        }
        if parts.is_empty() { "audio".to_string() } else { parts.join(" ") }
    }
}

/// Parse an SDP payload into [`StreamInfo`].
///
/// Unknown or absent fields are left empty rather than guessed. Tolerates the
/// RTSP response headers being present before the body.
pub fn parse(sdp: &str) -> StreamInfo {
    let mut info = StreamInfo::default();

    for (key, value) in lines(sdp) {
        match key {
            "s" => info.session = value.trim().to_string(),
            "c" => {
                // c=IN IP4 239.1.44.117/1  -> keep the address, drop the TTL.
                if let Some((_, addr, _)) = connection_fields(value) {
                    info.destination = addr.to_string();
                }
            }
            "m" => {
                // m=audio 5004 RTP/AVP 97
                let mut it = value.split_whitespace();
                if let Some(media) = it.next() {
                    info.is_audio = media.eq_ignore_ascii_case("audio");
                }
                if let Some(port) = it.next() {
                    info.rtp_port = port.parse().unwrap_or(0);
                }
            }
            "a" => {
                if let Some(rest) = value.strip_prefix("rtpmap:") {
                    if let Some((_, enc, rate, channels)) = rtpmap_fields(rest) {
                        info.codec = enc.to_string();
                        info.sample_rate = rate.unwrap_or(0);
                        info.channels = channels;
                    }
                } else if let Some(rest) = value.strip_prefix("ptime:") {
                    info.ptime_ms = rest.trim().to_string();
                } else if let Some(rest) = value.strip_prefix("clock-domain:") {
                    info.clock_domain = rest.trim().to_string();
                } else if let Some(rest) = value.strip_prefix("ts-refclk:") {
                    info.refclk = rest.trim().to_string();
                } else if ["recvonly", "sendonly", "sendrecv", "inactive"]
                    .contains(&value.trim())
                {
                    info.direction = value.trim().to_string();
                }
            }
            _ => {}
        }
    }
    info
}

// ------------------------------------------------- the strict reader + writer

/// The two linear PCM payloads AES67 allows (RFC 3190 / RFC 3551): big-endian
/// two's complement, interleaved by channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Encoding {
    L16,
    L24,
}

impl Encoding {
    /// Bytes per sample on the wire.
    pub fn bytes(self) -> usize {
        match self {
            Encoding::L16 => 2,
            Encoding::L24 => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Encoding::L16 => "L16",
            Encoding::L24 => "L24",
        }
    }

    pub fn parse(word: &str) -> Option<Encoding> {
        match word.trim().to_ascii_uppercase().as_str() {
            "L16" => Some(Encoding::L16),
            "L24" => Some(Encoding::L24),
            _ => None,
        }
    }
}

/// Which profile a stream description follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Profile {
    #[cfg_attr(feature = "serde", serde(rename = "aes67"))]
    Aes67,
    #[cfg_attr(feature = "serde", serde(rename = "st2110-30"))]
    St2110_30,
}

impl Profile {
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Aes67 => "aes67",
            Profile::St2110_30 => "st2110-30",
        }
    }
}

/// One stream, as either end of it needs to know it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StreamDesc {
    pub name: String,
    pub info: String,
    pub profile: Profile,
    pub origin_ip: Ipv4Addr,
    pub session_id: u64,
    pub session_version: u64,
    pub address: Ipv4Addr,
    pub port: u16,
    pub ttl: u8,
    pub payload_type: u8,
    pub encoding: Encoding,
    pub rate: u32,
    pub channels: u16,
    pub ptime_us: u32,
    /// `a=source-filter: incl` — the one sender a receiver should accept.
    pub source_filter: Option<Ipv4Addr>,
    /// `a=ts-refclk:` verbatim after the colon, e.g. `ptp=IEEE1588-2008:…:0`.
    pub refclk: Option<String>,
    /// `a=mediaclk:direct=<offset>` — RTP timestamp at the PTP epoch.
    pub mediaclk_offset: Option<u32>,
    /// `channel-order=` from fmtp, ST 2110-30 only.
    pub channel_order: Option<String>,
}

/// ST 2110-30 §6.2.1 — the level a (rate, ptime, channels) triple needs.
/// `None` means no level admits it, which a 2110 sender must not emit.
pub fn conformance_level(rate: u32, ptime_us: u32, channels: u16) -> Option<&'static str> {
    match (rate, ptime_us) {
        (48_000, 1000) if channels <= 8 => Some("A"),
        (96_000, 1000) if channels <= 4 => Some("AX"),
        (48_000, 125) if channels <= 8 => Some("B"),
        (96_000, 125) if channels <= 8 => Some("BX"),
        (48_000, 125) if channels <= 64 => Some("C"),
        (96_000, 125) if channels <= 32 => Some("CX"),
        _ => None,
    }
}

/// `SMPTE2110.(ST)` for a pair, `(M)` for one, `(U08)` otherwise — the
/// "undefined grouping" symbol, which claims nothing about the channels that a
/// bridge cannot know about the card they came from.
pub fn channel_order(channels: u16) -> String {
    match channels {
        1 => "SMPTE2110.(M)".to_string(),
        2 => "SMPTE2110.(ST)".to_string(),
        n => format!("SMPTE2110.(U{n:02})"),
    }
}

fn ptime_text(ptime_us: u32) -> String {
    if ptime_us.is_multiple_of(1000) {
        format!("{}", ptime_us / 1000)
    } else {
        let s = format!("{:.3}", ptime_us as f64 / 1000.0);
        s.trim_end_matches('0').to_string()
    }
}

/// The SDP a sender publishes (SAP, NMOS, an HTTP `…/sdp` endpoint).
pub fn generate(d: &StreamDesc) -> String {
    let mut s = String::new();
    s.push_str("v=0\r\n");
    s.push_str(&format!("o=- {} {} IN IP4 {}\r\n", d.session_id, d.session_version, d.origin_ip));
    s.push_str(&format!("s={}\r\n", d.name));
    if !d.info.is_empty() {
        s.push_str(&format!("i={}\r\n", d.info));
    }
    s.push_str(&format!("c=IN IP4 {}/{}\r\n", d.address, d.ttl));
    s.push_str("t=0 0\r\n");
    if d.profile == Profile::St2110_30 {
        s.push_str("a=recvonly\r\n");
    }
    s.push_str(&format!("m=audio {} RTP/AVP {}\r\n", d.port, d.payload_type));
    s.push_str(&format!("c=IN IP4 {}/{}\r\n", d.address, d.ttl));
    if let Some(src) = d.source_filter {
        s.push_str(&format!("a=source-filter: incl IN IP4 {} {}\r\n", d.address, src));
    }
    s.push_str(&format!("a=rtpmap:{} {}/{}/{}\r\n", d.payload_type, d.encoding.as_str(), d.rate, d.channels));
    if let Some(order) = &d.channel_order {
        s.push_str(&format!("a=fmtp:{} channel-order={}\r\n", d.payload_type, order));
    }
    s.push_str(&format!("a=ptime:{}\r\n", ptime_text(d.ptime_us)));
    s.push_str(&format!("a=maxptime:{}\r\n", ptime_text(d.ptime_us)));
    if let Some(refclk) = &d.refclk {
        s.push_str(&format!("a=ts-refclk:{refclk}\r\n"));
    }
    s.push_str(&format!("a=mediaclk:direct={}\r\n", d.mediaclk_offset.unwrap_or(0)));
    if d.profile == Profile::Aes67 {
        s.push_str("a=recvonly\r\n");
    }
    s
}

/// Parse the first audio media section of an SDP strictly. Errors name the
/// line that was missing or wrong, because they are shown to a person.
pub fn parse_stream(text: &str) -> Result<StreamDesc, String> {
    let mut d = StreamDesc {
        name: String::new(),
        info: String::new(),
        profile: Profile::Aes67,
        origin_ip: Ipv4Addr::UNSPECIFIED,
        session_id: 0,
        session_version: 0,
        address: Ipv4Addr::UNSPECIFIED,
        port: 0,
        ttl: 0,
        payload_type: 255,
        encoding: Encoding::L24,
        rate: 0,
        channels: 0,
        ptime_us: 1000,
        source_filter: None,
        refclk: None,
        mediaclk_offset: None,
        channel_order: None,
    };
    let mut in_audio = false;
    let mut seen_audio = false;
    let mut session_c: Option<(Ipv4Addr, u8)> = None;
    let mut media_c: Option<(Ipv4Addr, u8)> = None;
    let mut rtpmap_seen = false;

    for (kind, value) in lines(text) {
        match kind {
            "o" if !seen_audio => {
                let f: Vec<&str> = value.split_whitespace().collect();
                if f.len() >= 6 {
                    d.session_id = f[1].parse().unwrap_or(0);
                    d.session_version = f[2].parse().unwrap_or(0);
                    d.origin_ip = f[5].parse().unwrap_or(Ipv4Addr::UNSPECIFIED);
                }
            }
            "s" if !seen_audio => d.name = value.trim().to_string(),
            "i" if !seen_audio => d.info = value.trim().to_string(),
            "c" => {
                let c = parse_connection(value)?;
                if in_audio {
                    media_c = Some(c);
                } else if !seen_audio {
                    session_c = Some(c);
                }
            }
            "m" => {
                if seen_audio {
                    // Only the first audio section; a second is a different stream.
                    in_audio = false;
                    continue;
                }
                let f: Vec<&str> = value.split_whitespace().collect();
                if f.first() == Some(&"audio") && f.len() >= 4 {
                    in_audio = true;
                    seen_audio = true;
                    d.port = f[1].split('/').next().unwrap_or("0").parse().map_err(|_| format!("bad port in m={value}"))?;
                    d.payload_type = f[3].parse().map_err(|_| format!("bad payload type in m={value}"))?;
                } else {
                    in_audio = false;
                }
            }
            "a" if in_audio || !seen_audio => {
                let (attr, arg) = value.split_once(':').unwrap_or((value, ""));
                match attr {
                    "rtpmap" if in_audio => {
                        let (pt, enc, rate, channels) = rtpmap_fields(arg).ok_or("a=rtpmap without an encoding")?;
                        if pt.parse::<u8>().ok() != Some(d.payload_type) {
                            continue;
                        }
                        d.encoding = match Encoding::parse(enc) {
                            Some(e) => e,
                            None if enc.eq_ignore_ascii_case("AM824") => {
                                return Err("AM824 (ST 2110-31) is not supported; use L16 or L24 (ST 2110-30)".into())
                            }
                            None => return Err(format!("encoding {enc} is not L16 or L24")),
                        };
                        d.rate = rate.ok_or("a=rtpmap without a rate")?;
                        d.channels = u16::try_from(channels).unwrap_or(0);
                        rtpmap_seen = true;
                    }
                    "fmtp" if in_audio => {
                        if let Some((_, params)) = arg.split_once(' ') {
                            for p in params.split(';') {
                                if let Some(order) = p.trim().strip_prefix("channel-order=") {
                                    d.channel_order = Some(order.to_string());
                                    d.profile = Profile::St2110_30;
                                }
                            }
                        }
                    }
                    "ptime" => {
                        let ms: f64 = arg.trim().parse().map_err(|_| format!("bad a=ptime:{arg}"))?;
                        d.ptime_us = (ms * 1000.0).round() as u32;
                    }
                    "source-filter" => {
                        // a=source-filter: incl IN IP4 <dest> <src>...
                        let f: Vec<&str> = arg.split_whitespace().collect();
                        if f.first() == Some(&"incl") && f.len() >= 5 {
                            d.source_filter = f[4].parse().ok();
                        }
                    }
                    "ts-refclk" => d.refclk = Some(arg.trim().to_string()),
                    "mediaclk" => {
                        if let Some(off) = arg.trim().strip_prefix("direct=") {
                            d.mediaclk_offset = off.split_whitespace().next().and_then(|o| o.parse().ok());
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if !seen_audio {
        return Err("no m=audio section".into());
    }
    if !rtpmap_seen {
        return Err(format!("no a=rtpmap for payload type {}", d.payload_type));
    }
    let (address, ttl) = media_c.or(session_c).ok_or("no c= connection line")?;
    d.address = address;
    d.ttl = ttl;
    if d.channels == 0 || d.channels > 64 {
        return Err(format!("{} channels is outside 1..64", d.channels));
    }
    if d.ptime_us == 0 {
        return Err("ptime is zero".into());
    }
    Ok(d)
}

fn parse_connection(value: &str) -> Result<(Ipv4Addr, u8), String> {
    let (family, addr, ttl) = connection_fields(value).ok_or_else(|| format!("c={value} is not IN IP4"))?;
    if family != "IP4" {
        return Err(format!("c={value} is not IN IP4"));
    }
    let ip = addr.parse().map_err(|_| format!("bad address in c={value}"))?;
    let ttl = ttl.and_then(|t| t.parse().ok()).unwrap_or(0);
    Ok((ip, ttl))
}

// ------------------------------------------------------------ the corpus

/// One announcement from the fixture corpus (`SRC/vectors/sdp/`), with what
/// both readers must make of it. Consumers run it from their own tests
/// (`for case in apkaudio_ravenna::sdp::corpus()`), so a pinned rev that reads
/// a real announcement differently fails in the consumer, not on the bench.
pub struct Case {
    /// File name in `SRC/vectors/sdp/`.
    pub name: &'static str,
    /// Where the announcement came from (bench device, vendor, delivery path).
    pub source: &'static str,
    pub sdp: &'static str,
    /// `parse(sdp).format_summary()`.
    pub summary: &'static str,
    /// `parse(sdp).session`, `.destination`, `.rtp_port`, `.clock_domain`.
    pub session: &'static str,
    pub destination: &'static str,
    pub rtp_port: u16,
    pub clock_domain: &'static str,
    /// `parse_stream(sdp)`: `Ok((rate, channels, ptime_us))` or the start of
    /// the error text.
    pub stream: Result<(u32, u16, u32), &'static str>,
}

macro_rules! case {
    ($file:literal, $source:literal, $summary:literal, $session:literal, $dest:literal, $port:literal, $domain:literal, $stream:expr) => {
        Case {
            name: $file,
            source: $source,
            sdp: include_str!(concat!("../vectors/sdp/", $file)),
            summary: $summary,
            session: $session,
            destination: $dest,
            rtp_port: $port,
            clock_domain: $domain,
            stream: $stream,
        }
    };
}

/// The fixture corpus: real announcements from the bench and the vendors the
/// plugins meet, each with its expected parse.
pub fn corpus() -> Vec<Case> {
    vec![
        case!("merging-desk-premo.rtsp", "Merging/Neumann desk, RTSP DESCRIBE reply (RAVENNA)",
              "L24 48000Hz 2ch", "Desk preMO", "239.1.44.117", 5004, "PTPv2 0", Ok((48_000, 2, 1000))),
        case!("dante-aes67.sdp", "Dante AES67 mode, SAP payload (session-level c=)",
              "L24 48000Hz 8ch", "Dante TX", "239.1.2.3", 5004, "", Ok((48_000, 8, 1000))),
        case!("st2110-30-level-b.sdp", "ST 2110-30 sender, level B, channel-order fmtp",
              "L24 48000Hz 2ch", "APK stereo", "239.69.1.2", 5004, "PTPv2 0", Ok((48_000, 2, 125))),
        case!("mono-l16.sdp", "Mono L16, rtpmap without a channel count",
              "L16 48000Hz 1ch", "Talkback", "239.69.9.9", 5004, "", Ok((48_000, 1, 1000))),
        case!("am824-st2110-31.sdp", "ST 2110-31 AM824 — described, but not playable here",
              "AM824 48000Hz 2ch", "AES3 pair", "239.69.3.1", 5004, "", Err("AM824")),
        case!("ip-camera-video.rtsp", "IP camera answering RTSP on 554 — not audio",
              "not an audio stream", "Front Door", "", 5006, "", Err("no m=audio")),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from 44.44.44.166 on the bench.
    const DESK_PREMO: &str = "\
RTSP/1.0 200 OK\r\nContent-Type: application/sdp\r\nContent-Length: 287\r\nCSeq: 1\r\n\r\n\
v=0\n\
o=- 5 0 IN IP4 44.44.44.166\n\
s=Desk preMO\n\
t=0 0\n\
a=clock-domain:PTPv2 0\n\
m=audio 5004 RTP/AVP 97\n\
c=IN IP4 239.1.44.117/1\n\
a=rtpmap:97 L24/48000/2\n\
a=recvonly\n\
a=framecount:48\n\
a=ptime:1\n\
a=ts-refclk:ptp=IEEE1588-2008:00-07-F5-FF-FE-00-54-72:0\n\
a=mediaclk:direct=0\n";

    #[test]
    fn parses_a_real_ravenna_announcement() {
        let s = parse(DESK_PREMO);
        assert_eq!(s.session, "Desk preMO");
        assert_eq!(s.codec, "L24");
        assert_eq!(s.sample_rate, 48000);
        assert_eq!(s.channels, 2);
        assert_eq!(s.destination, "239.1.44.117"); // TTL suffix stripped
        assert_eq!(s.rtp_port, 5004);
        assert_eq!(s.ptime_ms, "1");
        assert_eq!(s.clock_domain, "PTPv2 0");
        assert_eq!(s.direction, "recvonly");
        assert!(s.refclk.starts_with("ptp=IEEE1588-2008"));
        assert!(s.is_audio);
        assert_eq!(s.format_summary(), "L24 48000Hz 2ch");
    }

    /// The discriminator that stops an IP camera being filed as an audio node.
    #[test]
    fn video_streams_are_not_audio() {
        let s = parse("v=0\ns=Front Door\nm=video 5006 RTP/AVP 96\na=rtpmap:96 H264/90000\n");
        assert!(!s.is_audio);
        assert_eq!(s.format_summary(), "not an audio stream");
    }

    /// Mono announcements omit the trailing channel count.
    #[test]
    fn mono_defaults_to_one_channel() {
        let s = parse("m=audio 5004 RTP/AVP 96\na=rtpmap:96 L24/48000\n");
        assert_eq!(s.channels, 1);
        assert_eq!(s.sample_rate, 48000);
    }

    #[test]
    fn missing_fields_stay_empty_rather_than_guessed() {
        let s = parse("v=0\ns=Bare\n");
        assert_eq!(s.session, "Bare");
        assert_eq!(s.codec, "");
        assert_eq!(s.sample_rate, 0);
        assert!(!s.is_audio);
    }

    fn sample(profile: Profile) -> StreamDesc {
        StreamDesc {
            name: "APK stereo".into(),
            info: "2 channels".into(),
            profile,
            origin_ip: "192.168.1.20".parse().unwrap(),
            session_id: 1234,
            session_version: 5,
            address: "239.69.1.2".parse().unwrap(),
            port: 5004,
            ttl: 32,
            payload_type: 97,
            encoding: Encoding::L24,
            rate: 48_000,
            channels: 2,
            ptime_us: 125,
            source_filter: Some("192.168.1.20".parse().unwrap()),
            refclk: Some("ptp=IEEE1588-2008:00-1D-C1-FF-FE-11-22-33:0".into()),
            mediaclk_offset: Some(0),
            channel_order: (profile == Profile::St2110_30).then(|| channel_order(2)),
        }
    }

    #[test]
    fn both_profiles_round_trip() {
        for p in [Profile::Aes67, Profile::St2110_30] {
            let d = sample(p);
            let back = parse_stream(&generate(&d)).unwrap();
            assert_eq!(back, d, "{}", generate(&d));
        }
    }

    /// What one plugin generates, every other plugin's discovery reader reads.
    #[test]
    fn the_discovery_reader_reads_what_the_writer_wrote() {
        let s = parse(&generate(&sample(Profile::Aes67)));
        assert_eq!((s.session.as_str(), s.sample_rate, s.channels), ("APK stereo", 48_000, 2));
        assert_eq!((s.destination.as_str(), s.rtp_port, s.ptime_ms.as_str()), ("239.69.1.2", 5004, "0.125"));
    }

    #[test]
    fn a_dante_style_session_level_connection_is_accepted() {
        let sdp = "v=0\no=- 1 2 IN IP4 10.0.0.5\ns=Dante TX\nc=IN IP4 239.1.2.3/32\nt=0 0\n\
                   m=audio 5004 RTP/AVP 97\na=rtpmap:97 L24/48000/8\na=ptime:1\n";
        let d = parse_stream(sdp).unwrap();
        assert_eq!(d.address, "239.1.2.3".parse::<Ipv4Addr>().unwrap());
        assert_eq!((d.channels, d.ptime_us, d.profile), (8, 1000, Profile::Aes67));
    }

    #[test]
    fn am824_is_refused_by_name() {
        let sdp = "v=0\nc=IN IP4 239.1.2.3/32\nm=audio 5004 RTP/AVP 97\na=rtpmap:97 AM824/48000/2\n";
        assert!(parse_stream(sdp).unwrap_err().contains("2110-31"));
    }

    #[test]
    fn levels_bound_channels_by_ptime() {
        assert_eq!(conformance_level(48_000, 1000, 8), Some("A"));
        assert_eq!(conformance_level(48_000, 125, 64), Some("C"));
        assert_eq!(conformance_level(48_000, 1000, 16), None);
    }

    /// The corpus is the contract consumers re-run; it must hold here first.
    #[test]
    fn the_corpus_parses_as_declared() {
        let cases = corpus();
        assert!(cases.len() >= 6);
        for c in cases {
            let s = parse(c.sdp);
            assert_eq!(s.format_summary(), c.summary, "{}", c.name);
            assert_eq!(s.session, c.session, "{}", c.name);
            assert_eq!(s.destination, c.destination, "{}", c.name);
            assert_eq!(s.rtp_port, c.rtp_port, "{}", c.name);
            assert_eq!(s.clock_domain, c.clock_domain, "{}", c.name);
            match (parse_stream(c.sdp), c.stream) {
                (Ok(d), Ok((rate, ch, ptime))) => assert_eq!((d.rate, d.channels, d.ptime_us), (rate, ch, ptime), "{}", c.name),
                (Err(e), Err(start)) => assert!(e.contains(start), "{}: {e}", c.name),
                (got, want) => panic!("{}: parse_stream gave {got:?}, corpus says {want:?}", c.name),
            }
        }
    }
}
