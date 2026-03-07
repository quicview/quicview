use crate::error::SessionError;

/// An opaque session token used for authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToken(Vec<u8>);

impl SessionToken {
    /// Create a token from raw bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Access the raw token bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Trait for validating session tokens.
///
/// The host runs a [`TokenValidator`] to decide whether an incoming
/// connection is allowed. Implementations can check pre-shared keys,
/// signed JWTs, TOTP codes, etc.
pub trait TokenValidator: Send + Sync {
    /// Validate the token. Returns `Ok(())` on success.
    fn validate(&self, token: &SessionToken) -> Result<(), SessionError>;
}

/// A validator that accepts every token. Suitable for local / trusted
/// networks or during development.
pub struct AcceptAll;

impl TokenValidator for AcceptAll {
    fn validate(&self, _token: &SessionToken) -> Result<(), SessionError> {
        Ok(())
    }
}

/// A simple pre-shared-key validator.
pub struct PresharedKeyValidator {
    expected: Vec<u8>,
}

impl PresharedKeyValidator {
    pub fn new(key: Vec<u8>) -> Self {
        Self { expected: key }
    }
}

impl TokenValidator for PresharedKeyValidator {
    fn validate(&self, token: &SessionToken) -> Result<(), SessionError> {
        // Constant-time comparison to avoid timing side-channels.
        if constant_time_eq(token.as_bytes(), &self.expected) {
            Ok(())
        } else {
            Err(SessionError::AuthFailed("invalid token".into()))
        }
    }
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_all_passes() {
        let v = AcceptAll;
        let token = SessionToken::new(b"anything".to_vec());
        assert!(v.validate(&token).is_ok());
    }

    #[test]
    fn psk_validator_accepts_correct() {
        let v = PresharedKeyValidator::new(b"secret".to_vec());
        let token = SessionToken::new(b"secret".to_vec());
        assert!(v.validate(&token).is_ok());
    }

    #[test]
    fn psk_validator_rejects_wrong() {
        let v = PresharedKeyValidator::new(b"secret".to_vec());
        let token = SessionToken::new(b"wrong".to_vec());
        assert!(v.validate(&token).is_err());
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hi", b"hello"));
    }
}
