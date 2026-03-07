use tokio::sync::mpsc;
use quicview_protocol::InputEvent;

use crate::error::InputError;

/// Forwards local input events captured on the viewer side to the host.
///
/// The viewer calls [`InputForwarder::send`] with captured UI events;
/// the receiving end (accessible via [`InputForwarder::receiver`]) is
/// read by the session layer and transmitted over the QUIC stream.
pub struct InputForwarder {
    tx: mpsc::Sender<InputEvent>,
    rx: Option<mpsc::Receiver<InputEvent>>,
}

impl InputForwarder {
    /// Create a forwarder with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx: Some(rx) }
    }

    /// Send an input event into the forwarding channel.
    pub async fn send(&self, event: InputEvent) -> Result<(), InputError> {
        self.tx
            .send(event)
            .await
            .map_err(|_| InputError::ChannelClosed)
    }

    /// Take the receiving half. Can only be called once; subsequent calls
    /// return `None`.
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<InputEvent>> {
        self.rx.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quicview_protocol::{KeyAction, MouseButton, MouseEvent};

    #[tokio::test]
    async fn forwarder_send_receive() {
        let mut fwd = InputForwarder::new(16);
        let mut rx = fwd.take_receiver().unwrap();

        let event = InputEvent::Mouse(MouseEvent {
            x: 50,
            y: 75,
            button: Some((MouseButton::Right, KeyAction::Press)),
        });

        fwd.send(event.clone()).await.unwrap();

        let got = rx.recv().await.unwrap();
        match got {
            InputEvent::Mouse(m) => {
                assert_eq!(m.x, 50);
                assert_eq!(m.y, 75);
            }
            _ => panic!("expected mouse event"),
        }
    }

    #[test]
    fn take_receiver_only_once() {
        let mut fwd = InputForwarder::new(4);
        assert!(fwd.take_receiver().is_some());
        assert!(fwd.take_receiver().is_none());
    }
}
