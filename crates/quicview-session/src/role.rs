use serde::{Deserialize, Serialize};

/// The role a participant plays in a QuicView session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Shares one or more displays and accepts remote input.
    Host,
    /// Views displayed content and sends input events.
    Viewer,
    /// Acts as both host and viewer — extends its own desktop onto
    /// a remote device's virtual display (the IoT / RPi use-case).
    Extender,
}

impl Role {
    /// Returns `true` if this role produces frames (captures display).
    pub fn is_producer(&self) -> bool {
        matches!(self, Role::Host | Role::Extender)
    }

    /// Returns `true` if this role consumes frames (renders display).
    pub fn is_consumer(&self) -> bool {
        matches!(self, Role::Viewer | Role::Extender)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Host => write!(f, "host"),
            Role::Viewer => write!(f, "viewer"),
            Role::Extender => write!(f, "extender"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_producer_consumer() {
        assert!(Role::Host.is_producer());
        assert!(!Role::Host.is_consumer());

        assert!(!Role::Viewer.is_producer());
        assert!(Role::Viewer.is_consumer());

        assert!(Role::Extender.is_producer());
        assert!(Role::Extender.is_consumer());
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Host.to_string(), "host");
        assert_eq!(Role::Viewer.to_string(), "viewer");
        assert_eq!(Role::Extender.to_string(), "extender");
    }

    #[test]
    fn role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Extender).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Role::Extender);
    }
}
