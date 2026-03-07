use std::mem;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
};

const XBUTTON1: u16 = 1;
const XBUTTON2: u16 = 2;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use quicview_protocol::{InputEvent, KeyAction, MouseButton, ScrollEvent};

use crate::error::InputError;
use crate::injector::InputInjector;

/// Windows input injector using the `SendInput` API.
///
/// Translates [`InputEvent`] into Win32 `INPUT` structures and injects
/// them via `SendInput`. Requires the process to have UI access.
pub struct WindowsInputInjector {
    events_injected: u64,
}

impl WindowsInputInjector {
    pub fn new() -> Self {
        Self { events_injected: 0 }
    }

    pub fn events_injected(&self) -> u64 {
        self.events_injected
    }

    fn inject_mouse_move(&self, x: i32, y: i32) -> Result<(), InputError> {
        let (screen_w, screen_h) = screen_size()?;
        let norm_x = ((x as i64 * 65535) / screen_w as i64) as i32;
        let norm_y = ((y as i64 * 65535) / screen_h as i64) as i32;

        let mut input = zero_input();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi = MOUSEINPUT {
            dx: norm_x,
            dy: norm_y,
            mouseData: 0,
            dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
            time: 0,
            dwExtraInfo: 0,
        };
        send_one(&input)
    }

    fn inject_mouse_button(
        &self,
        button: MouseButton,
        action: KeyAction,
    ) -> Result<(), InputError> {
        let (flags, mouse_data) = match (button, action) {
            (MouseButton::Left, KeyAction::Press) => (MOUSEEVENTF_LEFTDOWN, 0),
            (MouseButton::Left, KeyAction::Release) => (MOUSEEVENTF_LEFTUP, 0),
            (MouseButton::Right, KeyAction::Press) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (MouseButton::Right, KeyAction::Release) => (MOUSEEVENTF_RIGHTUP, 0),
            (MouseButton::Middle, KeyAction::Press) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (MouseButton::Middle, KeyAction::Release) => (MOUSEEVENTF_MIDDLEUP, 0),
            (MouseButton::Back, KeyAction::Press) => (MOUSEEVENTF_XDOWN, XBUTTON1 as u32),
            (MouseButton::Back, KeyAction::Release) => (MOUSEEVENTF_XUP, XBUTTON1 as u32),
            (MouseButton::Forward, KeyAction::Press) => (MOUSEEVENTF_XDOWN, XBUTTON2 as u32),
            (MouseButton::Forward, KeyAction::Release) => (MOUSEEVENTF_XUP, XBUTTON2 as u32),
        };

        let mut input = zero_input();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi = MOUSEINPUT {
            dx: 0,
            dy: 0,
            mouseData: mouse_data,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };
        send_one(&input)
    }

    fn inject_scroll(&self, scroll: &ScrollEvent) -> Result<(), InputError> {
        // Vertical scroll.
        if scroll.dy.abs() > f32::EPSILON {
            let wheel_delta = (-scroll.dy * 120.0) as i32;
            let mut input = zero_input();
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi = MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: wheel_delta as u32,
                dwFlags: MOUSEEVENTF_WHEEL,
                time: 0,
                dwExtraInfo: 0,
            };
            send_one(&input)?;
        }

        // Horizontal scroll.
        if scroll.dx.abs() > f32::EPSILON {
            let wheel_delta = (scroll.dx * 120.0) as i32;
            let mut input = zero_input();
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi = MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: wheel_delta as u32,
                dwFlags: MOUSEEVENTF_HWHEEL,
                time: 0,
                dwExtraInfo: 0,
            };
            send_one(&input)?;
        }

        Ok(())
    }

    fn inject_key(&self, keycode: u32, action: KeyAction) -> Result<(), InputError> {
        let flags = match action {
            KeyAction::Press => 0,
            KeyAction::Release => KEYEVENTF_KEYUP,
        };

        let mut input = zero_input();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki = KEYBDINPUT {
            wVk: keycode as u16,
            wScan: 0,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };
        send_one(&input)
    }
}

impl Default for WindowsInputInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl InputInjector for WindowsInputInjector {
    fn inject(&mut self, event: &InputEvent) -> Result<(), InputError> {
        match event {
            InputEvent::Mouse(mouse) => {
                self.inject_mouse_move(mouse.x, mouse.y)?;
                if let Some((button, action)) = &mouse.button {
                    self.inject_mouse_button(*button, *action)?;
                }
            }
            InputEvent::Key(key) => {
                self.inject_key(key.keycode, key.action)?;
            }
            InputEvent::Scroll(scroll) => {
                self.inject_scroll(scroll)?;
            }
        }
        self.events_injected += 1;
        Ok(())
    }
}

fn zero_input() -> INPUT {
    // SAFETY: INPUT is a plain data struct; zeroing is valid.
    unsafe { mem::zeroed() }
}

fn send_one(input: &INPUT) -> Result<(), InputError> {
    // SAFETY: We pass a valid INPUT struct and correct size.
    let sent = unsafe { SendInput(1, input, mem::size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err(InputError::InjectionFailed("SendInput returned 0".into()));
    }
    Ok(())
}

fn screen_size() -> Result<(i32, i32), InputError> {
    // SAFETY: GetSystemMetrics is safe with valid metric IDs.
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if w <= 0 || h <= 0 {
        return Err(InputError::InjectionFailed(
            "failed to get screen dimensions".into(),
        ));
    }
    Ok((w, h))
}
