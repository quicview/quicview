use crate::error::InputError;

/// Trait for capturing system audio.
pub trait AudioCapture: Send {
    /// Start capturing audio from the system's default output.
    fn start(&mut self) -> Result<(), InputError>;

    /// Read captured samples as interleaved f32 PCM.
    fn read_samples(&mut self) -> Result<Vec<f32>, InputError>;

    /// Stop capturing.
    fn stop(&mut self) -> Result<(), InputError>;
}

/// Stub audio capture that produces silence.
pub struct SilentAudioCapture {
    active: bool,
    sample_rate: u32,
    channels: u16,
}

impl SilentAudioCapture {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            active: false,
            sample_rate,
            channels,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl AudioCapture for SilentAudioCapture {
    fn start(&mut self) -> Result<(), InputError> {
        self.active = true;
        Ok(())
    }

    fn read_samples(&mut self) -> Result<Vec<f32>, InputError> {
        if !self.active {
            return Err(InputError::InjectionFailed("audio capture not started".into()));
        }
        // Return ~10ms of silence at the configured sample rate.
        let frame_count = self.sample_rate / 100;
        let total_samples = frame_count as usize * self.channels as usize;
        Ok(vec![0.0f32; total_samples])
    }

    fn stop(&mut self) -> Result<(), InputError> {
        self.active = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_audio_lifecycle() {
        let mut cap = SilentAudioCapture::new(48000, 2);
        assert!(cap.read_samples().is_err()); // not started

        cap.start().unwrap();
        let samples = cap.read_samples().unwrap();
        // 48000/100 = 480 frames × 2 channels = 960 samples.
        assert_eq!(samples.len(), 960);
        assert!(samples.iter().all(|&s| s == 0.0));

        cap.stop().unwrap();
    }
}
