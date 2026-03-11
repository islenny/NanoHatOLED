use anyhow::{Context, Result};
use rusttype::{Font, Scale, point};

use crate::framebuffer::FrameBuffer;

const DEJAVU_MONO_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSansMono.ttf");
const DEJAVU_MONO_BOLD: &[u8] = include_bytes!("../assets/DejaVuSansMono-Bold.ttf");

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FontFace {
    Regular,
    Bold,
}

pub struct FontSet {
    regular: Font<'static>,
    bold: Font<'static>,
}

impl FontSet {
    pub fn load() -> Result<Self> {
        let regular = Font::try_from_bytes(DEJAVU_MONO_REGULAR)
            .context("failed to parse DejaVuSansMono.ttf")?;
        let bold = Font::try_from_bytes(DEJAVU_MONO_BOLD)
            .context("failed to parse DejaVuSansMono-Bold.ttf")?;
        Ok(Self { regular, bold })
    }

    pub fn draw_text(
        &self,
        framebuffer: &mut FrameBuffer,
        x: i32,
        y: i32,
        text: &str,
        font_size_px: f32,
        face: FontFace,
        on: bool,
    ) {
        let font = match face {
            FontFace::Regular => &self.regular,
            FontFace::Bold => &self.bold,
        };

        let scale = Scale::uniform(font_size_px);
        let v_metrics = font.v_metrics(scale);
        let baseline = y as f32 + v_metrics.ascent;
        let glyphs = font.layout(text, scale, point(x as f32, baseline));

        for glyph in glyphs {
            if let Some(bounds) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, coverage| {
                    if coverage > 0.35 {
                        framebuffer.set_pixel(
                            bounds.min.x + gx as i32,
                            bounds.min.y + gy as i32,
                            on,
                        );
                    }
                });
            }
        }
    }

    pub fn measure_text(&self, text: &str, font_size_px: f32, face: FontFace) -> (i32, i32) {
        let font = match face {
            FontFace::Regular => &self.regular,
            FontFace::Bold => &self.bold,
        };

        let scale = Scale::uniform(font_size_px);
        let v_metrics = font.v_metrics(scale);
        let baseline = v_metrics.ascent;
        let glyphs = font.layout(text, scale, point(0.0, baseline));

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut found = false;

        for glyph in glyphs {
            if let Some(bounds) = glyph.pixel_bounding_box() {
                found = true;
                min_x = min_x.min(bounds.min.x);
                min_y = min_y.min(bounds.min.y);
                max_x = max_x.max(bounds.max.x);
                max_y = max_y.max(bounds.max.y);
            }
        }

        if !found {
            return (0, 0);
        }
        (max_x - min_x, max_y - min_y)
    }
}
