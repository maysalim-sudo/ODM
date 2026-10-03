/// A plain-language message for the UI. Never shows internal codes.
pub fn user_message(e: &odm_core::OdmError) -> String {
    use odm_core::ErrorKind::*;
    match e.kind {
        Http(404) => "The server could not find this file (error 404).".into(),
        Http(n @ (401 | 403)) => {
            format!("The server refused access (error {n}). You may need to sign in on the website first.")
        }
        Http(n) | HttpTransient(n) => format!("The server returned an error ({n})."),
        Timeout => "The server took too long to respond.".into(),
        Connection | Dns => {
            "Could not connect to the server. Check the address and your internet connection."
                .into()
        }
        DiskFull => "The disk is full.".into(),
        PermissionDenied => "ODM is not allowed to write to that folder.".into(),
        LinkExpired => "Link expired — re-send it from the browser.".into(),
        FileChanged => "The file changed on the server while it was downloading.".into(),
        ChecksumMismatch => "The downloaded file does not match the expected checksum.".into(),
        RangeIgnored => "The server does not support resuming this download.".into(),
        Cancelled => "Cancelled.".into(),
        Io | Other => {
            let m = e.message.trim();
            let mut chars = m.chars();
            let mut out: String = match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => "Something went wrong.".into(),
            };
            if !out.ends_with('.') {
                out.push('.');
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::user_message;
    use odm_core::{ErrorKind::*, OdmError};

    fn m(k: odm_core::ErrorKind, text: &str) -> String {
        user_message(&OdmError::new(k, text))
    }

    #[test]
    fn every_error_kind_has_a_plain_message() {
        assert_eq!(
            m(Http(404), "x"),
            "The server could not find this file (error 404)."
        );
        for n in [401, 403] {
            assert_eq!(
                m(Http(n), "x"),
                format!("The server refused access (error {n}). You may need to sign in on the website first.")
            );
        }
        assert_eq!(m(Http(410), "x"), "The server returned an error (410).");
        assert_eq!(
            m(HttpTransient(503), "x"),
            "The server returned an error (503)."
        );
        assert_eq!(m(Timeout, "x"), "The server took too long to respond.");
        for k in [Connection, Dns] {
            assert_eq!(
                m(k, "x"),
                "Could not connect to the server. Check the address and your internet connection."
            );
        }
        assert_eq!(m(DiskFull, "x"), "The disk is full.");
        assert_eq!(
            m(PermissionDenied, "x"),
            "ODM is not allowed to write to that folder."
        );
        assert_eq!(
            m(LinkExpired, "x"),
            "Link expired — re-send it from the browser."
        );
        assert_eq!(
            m(FileChanged, "x"),
            "The file changed on the server while it was downloading."
        );
        assert_eq!(
            m(ChecksumMismatch, "x"),
            "The downloaded file does not match the expected checksum."
        );
        assert_eq!(
            m(RangeIgnored, "x"),
            "The server does not support resuming this download."
        );
        assert_eq!(m(Cancelled, "x"), "Cancelled.");
        assert_eq!(m(Io, "disk went away"), "Disk went away.");
        assert_eq!(m(Other, "Already done."), "Already done.");
        assert_eq!(m(Other, ""), "Something went wrong.");
    }
}
