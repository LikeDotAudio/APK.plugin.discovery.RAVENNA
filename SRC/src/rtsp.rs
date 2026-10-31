// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The RAVENNA RTSP `/by-name/<session>` path — the one spelling both ends use.
//!
//! PLUGIN.RAVENNA's browse asks `DESCRIBE rtsp://<ip>:<port>/by-name/<session>`;
//! PLUGIN.AES67's RAVENNA advertisement serves that path. Session names are
//! operator-set and contain spaces and parentheses — `Digital inputs 1-2
//! (42-04-BA)` is a real one — so both must percent-encode them the same way,
//! and this is where that way lives.

/// Percent-encode a session name for the `/by-name/` path segment: RFC 3986
/// unreserved characters pass through, every other byte is `%XX`.
pub fn url_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The inverse of [`url_encode`], tolerant of `+` for a space and of a stray
/// `%` that is not followed by two hex digits (kept as written).
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `rtsp://<host>:<port>/by-name/<encoded session>`.
pub fn by_name_url(host: &str, port: u16, session: &str) -> String {
    format!("rtsp://{host}:{port}/by-name/{}", url_encode(session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_session_names_encode_and_round_trip() {
        assert_eq!(url_encode("Digital inputs 1-2 (42-04-BA)"), "Digital%20inputs%201-2%20%2842-04-BA%29");
        assert_eq!(url_encode("Desk preMO"), "Desk%20preMO");
        assert_eq!(url_encode("a-z_0.9~"), "a-z_0.9~");
        for n in ["bench TX 01", "Ü 1/2 + x", "a%b"] {
            assert_eq!(url_decode(&url_encode(n)), n);
        }
        assert_eq!(by_name_url("10.0.0.5", 554, "Desk preMO"), "rtsp://10.0.0.5:554/by-name/Desk%20preMO");
    }
}
