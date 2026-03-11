use image::GrayImage;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
pub const BUFFER_SIZE: usize = WIDTH * HEIGHT / 8;

#[derive(Clone)]
pub struct FrameBuffer {
    data: [u8; BUFFER_SIZE],
}

impl FrameBuffer {
    pub fn new() -> Self {
        Self {
            data: [0; BUFFER_SIZE],
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    pub fn raw(&self) -> &[u8] {
        &self.data
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as usize;
        let y = y as usize;
        if x >= WIDTH || y >= HEIGHT {
            return;
        }

        let page = y / 8;
        let bit = y % 8;
        let idx = page * WIDTH + x;
        let mask = 1u8 << bit;

        if on {
            self.data[idx] |= mask;
        } else {
            self.data[idx] &= !mask;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, on: bool) {
        if w <= 0 || h <= 0 {
            return;
        }
        for yy in y..(y + h) {
            for xx in x..(x + w) {
                self.set_pixel(xx, yy, on);
            }
        }
    }

    pub fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, on: bool) {
        if w <= 0 || h <= 0 {
            return;
        }
        for xx in x..(x + w) {
            self.set_pixel(xx, y, on);
            self.set_pixel(xx, y + h - 1, on);
        }
        for yy in y..(y + h) {
            self.set_pixel(x, yy, on);
            self.set_pixel(x + w - 1, yy, on);
        }
    }

    pub fn blit_luma_image(&mut self, image: &GrayImage, threshold: u8) {
        let w = image.width() as usize;
        let h = image.height() as usize;
        let offset_x = ((WIDTH.saturating_sub(w)) / 2) as i32;
        let offset_y = ((HEIGHT.saturating_sub(h)) / 2) as i32;

        for y in 0..h {
            for x in 0..w {
                let px = image.get_pixel(x as u32, y as u32)[0];
                self.set_pixel(offset_x + x as i32, offset_y + y as i32, px > threshold);
            }
        }
    }
}
