use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use std::time::Duration;

pub struct GifFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub delay: Duration,
}

pub fn decode(bytes: &[u8]) -> Result<Vec<GifFrame>, image::ImageError> {
    let decoder = GifDecoder::new(std::io::Cursor::new(bytes))?;
    let mut out = Vec::new();
    for frame in decoder.into_frames() {
        let frame = frame?;
        let (num, den) = frame.delay().numer_denom_ms();
        let ms = if den == 0 { 0 } else { num / den };
        let buf = frame.into_buffer();
        out.push(GifFrame {
            width: buf.width() as usize,
            height: buf.height() as usize,
            rgba: buf.into_raw(),
            delay: Duration::from_millis(if ms < 20 { 100 } else { ms as u64 }),
        });
    }
    Ok(out)
}
