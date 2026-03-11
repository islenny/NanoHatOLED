use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use gpio_cdev::{Chip, EventRequestFlags, LineRequestFlags};
use log::{error, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEvent {
    K1,
    K2,
    K3,
}

#[derive(Debug, Clone)]
pub struct ButtonConfig {
    pub chip_path: String,
    pub k1_line: u32,
    pub k2_line: u32,
    pub k3_line: u32,
}

pub fn spawn_button_listeners(
    config: &ButtonConfig,
    tx: Sender<ButtonEvent>,
) -> Vec<thread::JoinHandle<()>> {
    let mut handles = Vec::with_capacity(3);
    handles.push(spawn_listener(
        config.chip_path.clone(),
        config.k1_line,
        ButtonEvent::K1,
        tx.clone(),
    ));
    handles.push(spawn_listener(
        config.chip_path.clone(),
        config.k2_line,
        ButtonEvent::K2,
        tx.clone(),
    ));
    handles.push(spawn_listener(
        config.chip_path.clone(),
        config.k3_line,
        ButtonEvent::K3,
        tx,
    ));
    handles
}

fn spawn_listener(
    chip_path: String,
    line_offset: u32,
    button: ButtonEvent,
    tx: Sender<ButtonEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            if let Err(err) = listen_loop(&chip_path, line_offset, button, &tx) {
                error!(
                    "gpio listener failed: button={button:?}, chip={chip_path}, line={line_offset}, error={err:#}"
                );
                thread::sleep(Duration::from_secs(1));
            }
        }
    })
}

fn listen_loop(
    chip_path: &str,
    line_offset: u32,
    button: ButtonEvent,
    tx: &Sender<ButtonEvent>,
) -> Result<()> {
    let mut chip = Chip::new(chip_path).with_context(|| format!("open gpio chip {chip_path}"))?;
    let line = chip
        .get_line(line_offset)
        .with_context(|| format!("get gpio line {line_offset}"))?;

    let mut events = line
        .events(
            LineRequestFlags::INPUT,
            EventRequestFlags::RISING_EDGE,
            "nanohat-oled-rs",
        )
        .with_context(|| format!("request gpio event line {line_offset}"))?;

    info!("gpio listener started: button={button:?}, chip={chip_path}, line={line_offset}");

    loop {
        let _event = events
            .get_event()
            .with_context(|| format!("read gpio event line {line_offset}"))?;
        if tx.send(button).is_err() {
            return Ok(());
        }
    }
}
