use quicview_protocol::{DisplayId, NegotiateDisplay, Resolution, SessionOffer};

use crate::error::SessionError;

/// Tracks the state of a display negotiation handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationState {
    /// Waiting for the host's offer.
    AwaitingOffer,
    /// Offer received; waiting for the viewer to select displays.
    OfferReceived(SessionOffer),
    /// Viewer has selected displays; waiting for host acceptance.
    DisplaysSelected(Vec<NegotiateDisplay>),
    /// Negotiation complete — streaming can begin.
    Accepted,
    /// Negotiation was rejected.
    Rejected(String),
}

/// Drives the negotiation state machine.
pub struct Negotiator {
    state: NegotiationState,
}

impl Negotiator {
    pub fn new() -> Self {
        Self {
            state: NegotiationState::AwaitingOffer,
        }
    }

    pub fn state(&self) -> &NegotiationState {
        &self.state
    }

    /// Host sends an offer. Transitions `AwaitingOffer → OfferReceived`.
    pub fn receive_offer(
        &mut self,
        offer: SessionOffer,
    ) -> Result<(), SessionError> {
        if self.state != NegotiationState::AwaitingOffer {
            return Err(SessionError::NegotiationFailed(
                "unexpected offer in current state".into(),
            ));
        }
        self.state = NegotiationState::OfferReceived(offer);
        Ok(())
    }

    /// Viewer selects which displays to subscribe to and at what resolution.
    /// Transitions `OfferReceived → DisplaysSelected`.
    pub fn select_displays(
        &mut self,
        selections: Vec<NegotiateDisplay>,
    ) -> Result<(), SessionError> {
        match &self.state {
            NegotiationState::OfferReceived(_) => {
                self.state = NegotiationState::DisplaysSelected(selections);
                Ok(())
            }
            _ => Err(SessionError::NegotiationFailed(
                "must receive offer before selecting displays".into(),
            )),
        }
    }

    /// Host accepts the selection. Transitions `DisplaysSelected → Accepted`.
    pub fn accept(&mut self) -> Result<Vec<NegotiateDisplay>, SessionError> {
        match std::mem::replace(&mut self.state, NegotiationState::Accepted) {
            NegotiationState::DisplaysSelected(sel) => Ok(sel),
            other => {
                self.state = other;
                Err(SessionError::NegotiationFailed(
                    "nothing to accept".into(),
                ))
            }
        }
    }

    /// Either side rejects. Terminal state.
    pub fn reject(&mut self, reason: String) {
        self.state = NegotiationState::Rejected(reason);
    }

    /// Build a simple [`NegotiateDisplay`] for a given display at full resolution.
    pub fn full_resolution(
        display_id: DisplayId,
        resolution: Resolution,
    ) -> NegotiateDisplay {
        NegotiateDisplay {
            display_id: Some(display_id),
            resolution,
            refresh_hz: 60,
        }
    }
}

impl Default for Negotiator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quicview_protocol::{DisplayId, DisplayInfo, DisplayLayout, Resolution};

    fn sample_offer() -> SessionOffer {
        SessionOffer {
            host_name: "dev-machine".into(),
            displays: vec![DisplayInfo {
                id: DisplayId(0),
                name: "Primary".into(),
                resolution: Resolution::new(1920, 1080),
                refresh_hz: 60,
                is_virtual: false,
            }],
            layout: DisplayLayout { entries: vec![] },
            max_fps: 60,
            supports_virtual_display: true,
        }
    }

    #[test]
    fn negotiation_happy_path() {
        let mut neg = Negotiator::new();
        assert_eq!(*neg.state(), NegotiationState::AwaitingOffer);

        neg.receive_offer(sample_offer()).unwrap();
        assert!(matches!(neg.state(), NegotiationState::OfferReceived(_)));

        let sel = vec![Negotiator::full_resolution(
            DisplayId(0),
            Resolution::new(1920, 1080),
        )];
        neg.select_displays(sel).unwrap();
        assert!(matches!(neg.state(), NegotiationState::DisplaysSelected(_)));

        let accepted = neg.accept().unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(*neg.state(), NegotiationState::Accepted);
    }

    #[test]
    fn negotiation_reject() {
        let mut neg = Negotiator::new();
        neg.receive_offer(sample_offer()).unwrap();
        neg.reject("user cancelled".into());
        assert!(matches!(neg.state(), NegotiationState::Rejected(_)));
    }

    #[test]
    fn cannot_select_before_offer() {
        let mut neg = Negotiator::new();
        assert!(neg.select_displays(vec![]).is_err());
    }
}
