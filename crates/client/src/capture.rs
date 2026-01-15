// Capture module providing platform-specific frame sources.

#[cfg(feature = "macos-capture")]
pub mod macos {
    use image::{Rgb, RgbImage};
    use screenshots::Screen;
    use tokio::sync::mpsc;
    use tokio::time::{self, Duration};

    /// Spawn a background task that captures the primary display at ~`fps` frames per second.
    /// Returns a receiver yielding `RgbImage` frames.
    pub fn spawn(fps: u32) -> mpsc::Receiver<RgbImage> {
        let (tx, rx) = mpsc::channel(2);
        let interval = Duration::from_millis((1000 / fps.max(1)) as u64);
        tokio::spawn(async move {
            let screen = match Screen::all().ok().and_then(|v| v.into_iter().next()) {
                Some(s) => s,
                None => return,
            };
            let mut ticker = time::interval(interval);
            loop {
                ticker.tick().await;
                match screen.capture() {
                    Ok(img) => {
                        let (w, h) = (img.width(), img.height());
                        let buf = img.to_vec(); // BGRA8 contiguous
                        let mut rgb = RgbImage::new(w, h);
                        // Convert BGRA -> RGB
                        for y in 0..h {
                            let row_start = (y as usize) * (w as usize) * 4;
                            for x in 0..w {
                                let i = row_start + (x as usize) * 4;
                                let b = buf[i];
                                let g = buf[i + 1];
                                let r = buf[i + 2];
                                rgb.put_pixel(x, y, Rgb([r, g, b]));
                            }
                        }
                        if tx.send(rgb).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        rx
    }
}
