use std::net::SocketAddr;

use quinn::{Connection, Endpoint};

use crate::cert::SelfSignedCert;
use crate::error::TransportError;
use crate::mux::StreamMux;

/// A server-side QUIC listener that accepts incoming connections.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Bind a QUIC listener on the given address using the provided certificate.
    pub fn bind(addr: SocketAddr, cert: &SelfSignedCert) -> Result<Self, TransportError> {
        let server_config = cert.server_config()?;
        let endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| TransportError::BindFailed(e.to_string()))?;
        Ok(Self { endpoint })
    }

    /// Accept the next incoming connection.
    pub async fn accept(&self) -> Result<Connection, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| TransportError::AcceptFailed("endpoint closed".into()))?;
        let connection = incoming.await?;
        Ok(connection)
    }

    /// Create a stream multiplexer for an accepted connection.
    pub fn mux(connection: Connection) -> StreamMux {
        StreamMux::new(connection)
    }

    /// Local address the listener is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint
            .local_addr()
            .map_err(|e| TransportError::BindFailed(e.to_string()))
    }

    /// Access the underlying QUIC endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_and_get_local_addr() {
        let cert = SelfSignedCert::generate(&["localhost"]).unwrap();
        let listener =
            QuicListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)), &cert).unwrap();
        let addr = listener.local_addr().unwrap();
        assert_eq!(addr.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(addr.port(), 0);
    }
}
