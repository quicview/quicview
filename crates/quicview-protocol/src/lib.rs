//! # quicview-protocol
//!
//! Wire protocol definitions for QuicView visual streaming.
//!
//! Defines the frame types, input events, display metadata, and control
//! messages that travel between hosts and viewers over QUIC streams.

pub mod display;
pub mod error;
pub mod frame;
pub mod input;
pub mod message;

pub use display::{DisplayEntry, DisplayId, DisplayInfo, DisplayLayout, PixelFormat, Resolution};
pub use error::ProtocolError;
pub use frame::{FrameHeader, FrameKind};
pub use input::{InputEvent, KeyAction, KeyEvent, MouseButton, MouseEvent, ScrollEvent};
pub use message::{ControlMessage, NegotiateDisplay, SessionOffer};
