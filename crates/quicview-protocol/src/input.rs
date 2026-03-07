use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};

use crate::error::ProtocolError;

/// A mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// A key action (press or release).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyAction {
    Press,
    Release,
}

/// Mouse event with absolute coordinates in the display's virtual space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseEvent {
    /// Absolute X position within the display.
    pub x: i32,
    /// Absolute Y position within the display.
    pub y: i32,
    /// Button state change, if any.
    pub button: Option<(MouseButton, KeyAction)>,
}

/// Keyboard event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// Platform-independent key code (USB HID usage page 0x07).
    pub keycode: u32,
    /// Press or release.
    pub action: KeyAction,
    /// Active modifier flags (shift=1, ctrl=2, alt=4, meta=8).
    pub modifiers: u8,
}

/// Scroll / trackpad event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollEvent {
    /// Horizontal scroll delta (positive = right).
    pub dx: f32,
    /// Vertical scroll delta (positive = down).
    pub dy: f32,
}

/// Any input event that can be forwarded from viewer to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    Mouse(MouseEvent),
    Key(KeyEvent),
    Scroll(ScrollEvent),
}

/// Wire type tags for input events.
const TAG_MOUSE: u8 = 0;
const TAG_KEY: u8 = 1;
const TAG_SCROLL: u8 = 2;

impl InputEvent {
    /// Encode to a compact binary representation.
    pub fn encode(&self, dst: &mut BytesMut) {
        match self {
            Self::Mouse(m) => {
                dst.put_u8(TAG_MOUSE);
                dst.put_i32(m.x);
                dst.put_i32(m.y);
                match &m.button {
                    None => dst.put_u8(0xFF),
                    Some((btn, action)) => {
                        let btn_byte = match btn {
                            MouseButton::Left => 0,
                            MouseButton::Right => 1,
                            MouseButton::Middle => 2,
                            MouseButton::Back => 3,
                            MouseButton::Forward => 4,
                        };
                        let action_bit = match action {
                            KeyAction::Press => 0x80,
                            KeyAction::Release => 0x00,
                        };
                        dst.put_u8(btn_byte | action_bit);
                    }
                }
            }
            Self::Key(k) => {
                dst.put_u8(TAG_KEY);
                dst.put_u32(k.keycode);
                dst.put_u8(match k.action {
                    KeyAction::Press => 1,
                    KeyAction::Release => 0,
                });
                dst.put_u8(k.modifiers);
            }
            Self::Scroll(s) => {
                dst.put_u8(TAG_SCROLL);
                dst.put_f32(s.dx);
                dst.put_f32(s.dy);
            }
        }
    }

    /// Decode from binary.
    pub fn decode(src: &mut Bytes) -> Result<Self, ProtocolError> {
        if !src.has_remaining() {
            return Err(ProtocolError::Decode("empty input event".into()));
        }
        let tag = src.get_u8();
        match tag {
            TAG_MOUSE => {
                if src.remaining() < 9 {
                    return Err(ProtocolError::Decode("truncated mouse event".into()));
                }
                let x = src.get_i32();
                let y = src.get_i32();
                let btn_byte = src.get_u8();
                let button = if btn_byte == 0xFF {
                    None
                } else {
                    let btn = match btn_byte & 0x7F {
                        0 => MouseButton::Left,
                        1 => MouseButton::Right,
                        2 => MouseButton::Middle,
                        3 => MouseButton::Back,
                        4 => MouseButton::Forward,
                        v => {
                            return Err(ProtocolError::UnknownInputEvent(v));
                        }
                    };
                    let action = if btn_byte & 0x80 != 0 {
                        KeyAction::Press
                    } else {
                        KeyAction::Release
                    };
                    Some((btn, action))
                };
                Ok(Self::Mouse(MouseEvent { x, y, button }))
            }
            TAG_KEY => {
                if src.remaining() < 6 {
                    return Err(ProtocolError::Decode("truncated key event".into()));
                }
                let keycode = src.get_u32();
                let action = if src.get_u8() == 1 {
                    KeyAction::Press
                } else {
                    KeyAction::Release
                };
                let modifiers = src.get_u8();
                Ok(Self::Key(KeyEvent {
                    keycode,
                    action,
                    modifiers,
                }))
            }
            TAG_SCROLL => {
                if src.remaining() < 8 {
                    return Err(ProtocolError::Decode("truncated scroll event".into()));
                }
                let dx = src.get_f32();
                let dy = src.get_f32();
                Ok(Self::Scroll(ScrollEvent { dx, dy }))
            }
            other => Err(ProtocolError::UnknownInputEvent(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_event_roundtrip() {
        let event = InputEvent::Mouse(MouseEvent {
            x: 1920,
            y: 540,
            button: Some((MouseButton::Left, KeyAction::Press)),
        });

        let mut buf = BytesMut::new();
        event.encode(&mut buf);
        let mut bytes = buf.freeze();
        let decoded = InputEvent::decode(&mut bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn key_event_roundtrip() {
        let event = InputEvent::Key(KeyEvent {
            keycode: 0x04, // 'A' in HID
            action: KeyAction::Press,
            modifiers: 0x02, // ctrl
        });

        let mut buf = BytesMut::new();
        event.encode(&mut buf);
        let mut bytes = buf.freeze();
        let decoded = InputEvent::decode(&mut bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn scroll_event_roundtrip() {
        let event = InputEvent::Scroll(ScrollEvent { dx: 0.0, dy: -3.5 });

        let mut buf = BytesMut::new();
        event.encode(&mut buf);
        let mut bytes = buf.freeze();
        let decoded = InputEvent::decode(&mut bytes).unwrap();
        assert_eq!(decoded, event);
    }
}
