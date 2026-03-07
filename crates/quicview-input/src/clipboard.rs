use crate::error::InputError;

/// Trait for reading and writing the system clipboard.
pub trait ClipboardProvider: Send {
    /// Read UTF-8 text from the clipboard.
    fn read_text(&self) -> Result<String, InputError>;

    /// Write UTF-8 text to the clipboard.
    fn write_text(&mut self, text: &str) -> Result<(), InputError>;
}

/// Stub clipboard provider that stores text in memory (for testing).
pub struct MemoryClipboard {
    text: String,
}

impl MemoryClipboard {
    pub fn new() -> Self {
        Self {
            text: String::new(),
        }
    }
}

impl Default for MemoryClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardProvider for MemoryClipboard {
    fn read_text(&self) -> Result<String, InputError> {
        Ok(self.text.clone())
    }

    fn write_text(&mut self, text: &str) -> Result<(), InputError> {
        self.text = text.to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_clipboard_roundtrip() {
        let mut cb = MemoryClipboard::new();
        cb.write_text("hello clipboard").unwrap();
        assert_eq!(cb.read_text().unwrap(), "hello clipboard");
    }
}
