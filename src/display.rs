use anyhow::{Context, Result};
use embedded_hal::i2c::I2c;
use linux_embedded_hal::I2cdev;

use crate::framebuffer::FrameBuffer;

const CMD_CONTROL_BYTE: u8 = 0x00;
const DATA_CONTROL_BYTE: u8 = 0x40;

pub struct Ssd1306Display {
    i2c: I2cdev,
    address: u8,
}

impl Ssd1306Display {
    pub fn new(bus_path: &str, address: u8) -> Result<Self> {
        let i2c =
            I2cdev::new(bus_path).with_context(|| format!("failed to open i2c bus {bus_path}"))?;
        Ok(Self { i2c, address })
    }

    pub fn init(&mut self) -> Result<()> {
        let init_commands = [
            0xAE, // Display OFF
            0x00, 0x10, // Column low/high
            0x40, // Start line
            0xB0, // Page start
            0x81, 0xCF, // Contrast
            0xA1, // Segment remap
            0xA6, // Normal display
            0xA8, 0x3F, // Multiplex ratio
            0xC8, // COM scan direction
            0xD3, 0x00, // Display offset
            0xD5, 0x80, // Osc frequency
            0xD9, 0xF1, // Pre-charge
            0xDA, 0x12, // COM pins
            0xDB, 0x40, // VCOMH
            0x8D, 0x14, // Charge pump
            0xAF, // Display ON
        ];
        self.send_commands(&init_commands)?;
        self.set_horizontal_mode()?;
        Ok(())
    }

    pub fn set_normal_display(&mut self) -> Result<()> {
        self.send_command(0xA6)
    }

    pub fn set_horizontal_mode(&mut self) -> Result<()> {
        self.send_commands(&[0x20, 0x00])
    }

    pub fn clear_display(&mut self) -> Result<()> {
        let empty = FrameBuffer::new();
        self.draw_frame(&empty)
    }

    pub fn power_off(&mut self) -> Result<()> {
        self.send_command(0xAE)
    }

    pub fn power_on(&mut self) -> Result<()> {
        self.send_command(0xAF)
    }

    pub fn draw_frame(&mut self, frame: &FrameBuffer) -> Result<()> {
        // Set full column/page range.
        self.send_commands(&[0x21, 0x00, 0x7F, 0x22, 0x00, 0x07])?;

        for chunk in frame.raw().chunks(32) {
            let mut tx = [0u8; 33];
            tx[0] = DATA_CONTROL_BYTE;
            tx[1..(chunk.len() + 1)].copy_from_slice(chunk);
            self.i2c
                .write(self.address, &tx[..(chunk.len() + 1)])
                .context("failed to write display data")?;
        }
        Ok(())
    }

    fn send_commands(&mut self, commands: &[u8]) -> Result<()> {
        for &cmd in commands {
            self.send_command(cmd)?;
        }
        Ok(())
    }

    fn send_command(&mut self, command: u8) -> Result<()> {
        self.i2c
            .write(self.address, &[CMD_CONTROL_BYTE, command])
            .with_context(|| format!("failed to write command 0x{command:02x}"))?;
        Ok(())
    }
}
