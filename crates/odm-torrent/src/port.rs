//! Choosing a listen port that is free for both TCP and UDP. Windows reserves UDP port
//! ranges (Hyper-V, WinNAT) that TCP checks do not see, so both are tried.

pub const PORT_MIN: u16 = 49152;
pub const PORT_MAX: u16 = 65535;
const ATTEMPTS: usize = 8;

pub fn random_port() -> u16 {
    fastrand::u16(PORT_MIN..=PORT_MAX)
}

fn is_free(port: u16) -> bool {
    use std::net::{TcpListener, UdpSocket};
    // Both are dropped before returning, so the session can bind the port right after.
    TcpListener::bind(("0.0.0.0", port)).is_ok() && UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

/// `preferred` when both TCP and UDP can bind it, otherwise a random free port in range.
/// 0 means "choose one".
pub fn pick_port(preferred: u16) -> Option<u16> {
    if preferred != 0 && is_free(preferred) {
        return Some(preferred);
    }
    (0..ATTEMPTS)
        .map(|_| random_port())
        .find(|&p| p != preferred && is_free(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, UdpSocket};

    fn free() -> u16 {
        loop {
            let p = random_port();
            if TcpListener::bind(("0.0.0.0", p)).is_ok() && UdpSocket::bind(("0.0.0.0", p)).is_ok() {
                return p;
            }
        }
    }

    #[test]
    fn free_preferred_is_kept() {
        let p = free();
        assert_eq!(pick_port(p), Some(p));
    }

    #[test]
    fn busy_tcp_moves_on() {
        let p = free();
        let _hold = TcpListener::bind(("0.0.0.0", p)).unwrap();
        let got = pick_port(p).unwrap();
        assert_ne!(got, p);
        assert!((PORT_MIN..=PORT_MAX).contains(&got));
    }

    #[test]
    fn busy_udp_moves_on() {
        let p = free();
        let _hold = UdpSocket::bind(("0.0.0.0", p)).unwrap();
        let got = pick_port(p).unwrap();
        assert_ne!(got, p);
        assert!((PORT_MIN..=PORT_MAX).contains(&got));
    }

    #[test]
    fn random_in_range() {
        assert!((0..1000).all(|_| (PORT_MIN..=PORT_MAX).contains(&random_port())));
    }

    #[test]
    fn zero_means_choose() {
        let got = pick_port(0).unwrap();
        assert!((PORT_MIN..=PORT_MAX).contains(&got));
    }
}
