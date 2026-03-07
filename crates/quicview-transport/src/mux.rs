use quinn::{Connection, RecvStream, SendStream};

use crate::error::TransportError;

/// The kind of QUIC stream, identified by a 1-byte tag at stream open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StreamKind {
    /// Video frame data (host → viewer).
    Video = 0,
    /// Input events (viewer → host).
    Input = 1,
    /// Control messages (bidirectional negotiation, keep-alive).
    Control = 2,
}

impl StreamKind {
    fn from_tag(tag: u8) -> Result<Self, TransportError> {
        match tag {
            0 => Ok(Self::Video),
            1 => Ok(Self::Input),
            2 => Ok(Self::Control),
            _ => Err(TransportError::Protocol(
                quicview_protocol::ProtocolError::Decode(format!("unknown stream kind: {tag}")),
            )),
        }
    }
}

/// Multiplexes typed bidirectional streams over a single QUIC connection.
///
/// Each stream begins with a 1-byte tag identifying its [`StreamKind`].
/// The opener writes the tag; the acceptor reads it.
pub struct StreamMux {
    connection: Connection,
}

impl StreamMux {
    /// Create a new stream multiplexer over the given connection.
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Open a new bidirectional stream of the given kind.
    ///
    /// Writes the 1-byte stream kind tag before returning the streams.
    pub async fn open(
        &self,
        kind: StreamKind,
    ) -> Result<(SendStream, RecvStream), TransportError> {
        let (mut send, recv) = self
            .connection
            .open_bi()
            .await
            .map_err(|e| TransportError::StreamOpenFailed(e.to_string()))?;
        send.write_all(&[kind as u8])
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        Ok((send, recv))
    }

    /// Accept the next incoming bidirectional stream and read its kind tag.
    pub async fn accept(
        &self,
    ) -> Result<(StreamKind, SendStream, RecvStream), TransportError> {
        let (send, mut recv) = self.connection.accept_bi().await?;
        let mut tag = [0u8; 1];
        recv.read_exact(&mut tag)
            .await
            .map_err(|e| TransportError::RecvFailed(e.to_string()))?;
        let kind = StreamKind::from_tag(tag[0])?;
        Ok((kind, send, recv))
    }

    /// Access the underlying QUIC connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Remote address of the peer.
    pub fn remote_address(&self) -> std::net::SocketAddr {
        self.connection.remote_address()
    }
}
