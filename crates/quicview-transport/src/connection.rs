use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use quinn::{Connection, Endpoint};

use crate::cert::SelfSignedCert;
use crate::error::TransportError;
use crate::mux::StreamMux;

/// A client-side QUIC connection to a remote QuicView host.
pub struct QuicConnection {
    endpoint: Endpoint,
    connection: Connection,
}

impl QuicConnection {
    /// Connect to a remote QuicView host.
    ///
    /// Uses a self-signed client config that skips server certificate
    /// verification (suitable for LAN / development).
    pub async fn connect(
        remote: SocketAddr,
        server_name: &str,
    ) -> Result<Self, TransportError> {
        let client_config = SelfSignedCert::client_config()?;

        let bind_addr = if remote.is_ipv4() {
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)
        } else {
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0)
        };

        let mut endpoint = Endpoint::client(bind_addr)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(remote, server_name)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?
            .await?;

        Ok(Self {
            endpoint,
            connection,
        })
    }

    /// Create a stream multiplexer over this connection.
    pub fn mux(&self) -> StreamMux {
        StreamMux::new(self.connection.clone())
    }

    /// Remote address of the peer.
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Close the connection gracefully.
    pub fn close(&self) {
        self.connection
            .close(quinn::VarInt::from_u32(0), b"bye");
    }

    /// Access the underlying QUIC endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}
