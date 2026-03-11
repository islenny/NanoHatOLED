mod buttons;
mod display;
mod framebuffer;
mod metrics;
mod text;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Datelike, Local, TimeZone};
use crossbeam_channel::{RecvTimeoutError, unbounded};
use fs2::FileExt;
use image::{GrayImage, ImageReader};
use log::{Level, LevelFilter, Log, Metadata, Record, error, info, warn};
use nix::unistd::daemon as nix_daemon;

use crate::buttons::{ButtonConfig, ButtonEvent, spawn_button_listeners};
use crate::display::Ssd1306Display;
use crate::framebuffer::FrameBuffer;
use crate::metrics::SystemMetrics;
use crate::text::{FontFace, FontSet};

const SPLASH_PNG: &[u8] = include_bytes!("../assets/friendllyelec.png");
const PAGE0_LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Page {
    Logo,
    Clock,
    SystemInfo,
    MetricIp,
    MetricCpuLoad,
    MetricMemory,
    MetricDisk,
    MetricTemp,
    PowerMenuCancel,
    PowerMenuReboot,
    PowerMenuShutdown,
    Rebooting,
    ShuttingDown,
}

#[derive(Debug, Clone)]
struct Args {
    i2c_bus: String,
    i2c_address: u8,
    gpio_chip: String,
    k1_line: u32,
    k2_line: u32,
    k3_line: u32,
    log_file: PathBuf,
    pid_file: PathBuf,
    splash_secs: u64,
    shutdown_message_secs: u64,
    shutdown_wait_secs: u64,
    idle_power_off_secs: u64,
    disable_gpio: bool,
    dry_run_poweroff: bool,
    daemonize: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = Self {
            i2c_bus: env_or_default("NANOHAT_I2C_BUS", "/dev/i2c-0"),
            i2c_address: parse_u8_auto(&env_or_default("NANOHAT_I2C_ADDR", "0x3c"))
                .map_err(|e| anyhow!(e))?,
            gpio_chip: env_or_default("NANOHAT_GPIO_CHIP", "/dev/gpiochip1"),
            k1_line: env_parse_or_default("NANOHAT_K1_LINE", 0u32)?,
            k2_line: env_parse_or_default("NANOHAT_K2_LINE", 2u32)?,
            k3_line: env_parse_or_default("NANOHAT_K3_LINE", 3u32)?,
            log_file: PathBuf::from(env_or_default("NANOHAT_LOG_FILE", "/tmp/nanohat-oled.log")),
            pid_file: PathBuf::from(env_or_default("NANOHAT_PID_FILE", "/run/nanohat-oled.pid")),
            splash_secs: env_parse_or_default("NANOHAT_SPLASH_SECS", 2u64)?,
            shutdown_message_secs: env_parse_or_default("NANOHAT_SHUTDOWN_MSG_SECS", 2u64)?,
            shutdown_wait_secs: env_parse_or_default("NANOHAT_SHUTDOWN_WAIT_SECS", 1u64)?,
            idle_power_off_secs: env_parse_or_default("NANOHAT_IDLE_POWER_OFF_SECS", 30u64)?,
            disable_gpio: false,
            dry_run_poweroff: false,
            daemonize: false,
        };

        let mut cli = std::env::args().skip(1);
        while let Some(arg) = cli.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print_help();
                    process::exit(0);
                }
                "--i2c-bus" => args.i2c_bus = next_value(&mut cli, "--i2c-bus")?,
                "--i2c-addr" => {
                    let raw = next_value(&mut cli, "--i2c-addr")?;
                    args.i2c_address = parse_u8_auto(&raw).map_err(|e| anyhow!(e))?;
                }
                "--gpio-chip" => args.gpio_chip = next_value(&mut cli, "--gpio-chip")?,
                "--k1-line" => args.k1_line = parse_arg_u32(&mut cli, "--k1-line")?,
                "--k2-line" => args.k2_line = parse_arg_u32(&mut cli, "--k2-line")?,
                "--k3-line" => args.k3_line = parse_arg_u32(&mut cli, "--k3-line")?,
                "--log-file" => args.log_file = PathBuf::from(next_value(&mut cli, "--log-file")?),
                "--pid-file" => args.pid_file = PathBuf::from(next_value(&mut cli, "--pid-file")?),
                "--splash-secs" => args.splash_secs = parse_arg_u64(&mut cli, "--splash-secs")?,
                "--shutdown-msg-secs" => {
                    args.shutdown_message_secs = parse_arg_u64(&mut cli, "--shutdown-msg-secs")?
                }
                "--shutdown-wait-secs" => {
                    args.shutdown_wait_secs = parse_arg_u64(&mut cli, "--shutdown-wait-secs")?
                }
                "--idle-power-off-secs" => {
                    args.idle_power_off_secs = parse_arg_u64(&mut cli, "--idle-power-off-secs")?
                }
                "--disable-gpio" => args.disable_gpio = true,
                "--dry-run-poweroff" => args.dry_run_poweroff = true,
                "--daemonize" => args.daemonize = true,
                _ => bail!("unknown argument: {arg} (use --help)"),
            }
        }

        Ok(args)
    }
}

struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            );
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

fn print_help() {
    println!(
        "nanohat-oled-rs\n\
         \n\
         Options:\n\
           --i2c-bus <path>\n\
           --i2c-addr <u8|0x..>\n\
           --gpio-chip <path>\n\
           --k1-line <u32>\n\
           --k2-line <u32>\n\
           --k3-line <u32>\n\
           --log-file <path>\n\
           --pid-file <path>\n\
           --splash-secs <u64>\n\
           --shutdown-msg-secs <u64>\n\
           --shutdown-wait-secs <u64>\n\
           --idle-power-off-secs <u64>\n\
           --disable-gpio\n\
           --dry-run-poweroff\n\
           --daemonize\n\
           -h, --help\n\
         \n\
         Environment variables:\n\
           NANOHAT_I2C_BUS, NANOHAT_I2C_ADDR, NANOHAT_GPIO_CHIP,\n\
           NANOHAT_K1_LINE, NANOHAT_K2_LINE, NANOHAT_K3_LINE,\n\
           NANOHAT_LOG_FILE, NANOHAT_PID_FILE, NANOHAT_SPLASH_SECS,\n\
           NANOHAT_SHUTDOWN_MSG_SECS, NANOHAT_SHUTDOWN_WAIT_SECS,\n\
           NANOHAT_IDLE_POWER_OFF_SECS"
    );
}

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse_or_default<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr + Copy,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow!("invalid env {}={}: {}", key, raw, e)),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(e) => Err(anyhow!("failed to read env {}: {}", key, e)),
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| anyhow!("missing value for {}", name))
}

fn parse_arg_u32(args: &mut impl Iterator<Item = String>, name: &str) -> Result<u32> {
    let raw = next_value(args, name)?;
    raw.parse::<u32>()
        .map_err(|e| anyhow!("invalid value for {}: {} ({})", name, raw, e))
}

fn parse_arg_u64(args: &mut impl Iterator<Item = String>, name: &str) -> Result<u64> {
    let raw = next_value(args, name)?;
    raw.parse::<u64>()
        .map_err(|e| anyhow!("invalid value for {}: {} ({})", name, raw, e))
}

fn main() {
    if let Err(err) = run() {
        eprintln!("nanohat-oled-rs failed: {err:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse()?;

    if args.daemonize {
        daemonize_process()?;
    }

    init_logger(&args.log_file)?;
    info!("nanohat-oled-rs starting: {:?}", args);

    let _pid_lock = lock_single_instance(&args.pid_file)?;

    let mut display = Ssd1306Display::new(&args.i2c_bus, args.i2c_address)?;
    display.init()?;
    display.set_normal_display()?;
    display.set_horizontal_mode()?;

    let splash_image = load_embedded_png(SPLASH_PNG)?;
    let logo_image = load_embedded_png(PAGE0_LOGO_PNG)?;
    show_splash(&mut display, args.splash_secs, &splash_image)?;
    let fonts = FontSet::load()?;
    let version_codename = read_version_codename();

    let (tx, rx) = unbounded::<ButtonEvent>();
    let _button_threads = if args.disable_gpio {
        info!("gpio input disabled by flag");
        Vec::new()
    } else {
        let cfg = ButtonConfig {
            chip_path: args.gpio_chip.clone(),
            k1_line: args.k1_line,
            k2_line: args.k2_line,
            k3_line: args.k3_line,
        };
        spawn_button_listeners(&cfg, tx.clone())
    };

    let mut framebuffer = FrameBuffer::new();
    let mut page = Page::Clock;
    let mut redraw = true;
    let mut last_button: Option<(ButtonEvent, Instant)> = None;
    let mut last_input = Instant::now();
    let idle_power_off =
        (args.idle_power_off_secs > 0).then_some(Duration::from_secs(args.idle_power_off_secs));
    let mut display_sleeping = false;

    loop {
        if let Some(idle_timeout) = idle_power_off
            && !display_sleeping
            && !matches!(page, Page::ShuttingDown | Page::Rebooting)
            && Instant::now().duration_since(last_input) >= idle_timeout
        {
            info!(
                "idle timeout reached ({}s), power off OLED",
                args.idle_power_off_secs
            );
            display.power_off()?;
            display_sleeping = true;
        }

        if redraw && !display_sleeping {
            render_page(
                page,
                &mut framebuffer,
                &fonts,
                &logo_image,
                version_codename.as_deref(),
            );
            display.draw_frame(&framebuffer)?;
            redraw = false;
        }

        if matches!(page, Page::ShuttingDown | Page::Rebooting) {
            info!("enter power action flow: {page:?}");
            thread::sleep(Duration::from_secs(args.shutdown_message_secs));
            display.clear_display()?;
            thread::sleep(Duration::from_secs(args.shutdown_wait_secs));
            match page {
                Page::ShuttingDown => run_poweroff(args.dry_run_poweroff)?,
                Page::Rebooting => run_reboot(args.dry_run_poweroff)?,
                _ => {}
            }
            break;
        }

        let timeout = redraw_interval(page);
        match rx.recv_timeout(timeout) {
            Ok(button) => {
                let now = Instant::now();
                if let Some((last, ts)) = last_button
                    && last == button
                    && now.duration_since(ts) < Duration::from_millis(120)
                {
                    continue;
                }
                last_button = Some((button, now));
                last_input = now;

                if display_sleeping {
                    info!("button={button:?}, wake OLED");
                    display.power_on()?;
                    display_sleeping = false;
                    redraw = true;
                }

                let new_page = transition_page(page, button);
                if new_page != page {
                    info!("button={button:?}, page={page:?} -> {new_page:?}");
                    page = new_page;
                    redraw = true;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if !display_sleeping
                    && matches!(
                        page,
                        Page::Clock
                            | Page::SystemInfo
                            | Page::MetricIp
                            | Page::MetricCpuLoad
                            | Page::MetricMemory
                            | Page::MetricDisk
                            | Page::MetricTemp
                    )
                {
                    redraw = true;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("button channel disconnected unexpectedly");
            }
        }
    }

    info!("nanohat-oled-rs exit");
    Ok(())
}

fn daemonize_process() -> Result<()> {
    nix_daemon(false, false).context("daemonize failed")
}

fn init_logger(log_file: &Path) -> Result<()> {
    if let Some(parent) = log_file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log dir {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .with_context(|| format!("failed to open log file {}", log_file.display()))?;
    let logger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
    }));
    log::set_logger(logger).map_err(|_| anyhow!("failed to initialize logger"))?;
    log::set_max_level(LevelFilter::Info);
    Ok(())
}

fn lock_single_instance(pid_file: &Path) -> Result<File> {
    if let Some(parent) = pid_file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create pid dir {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(pid_file)
        .with_context(|| format!("failed to open pid file {}", pid_file.display()))?;

    file.try_lock_exclusive().with_context(|| {
        format!(
            "another instance is running (pid lock: {})",
            pid_file.display()
        )
    })?;

    file.set_len(0).context("failed to truncate pid file")?;
    writeln!(file, "{}", process::id()).context("failed to write pid file")?;
    Ok(file)
}

fn load_embedded_png(bytes: &[u8]) -> Result<GrayImage> {
    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .context("failed to detect image format")?
        .decode()
        .context("failed to decode image")?
        .to_luma8();
    Ok(image)
}

fn show_splash(display: &mut Ssd1306Display, seconds: u64, image: &GrayImage) -> Result<()> {
    if seconds == 0 {
        return Ok(());
    }

    let mut framebuffer = FrameBuffer::new();
    framebuffer.blit_luma_image(image, 127);
    display.draw_frame(&framebuffer)?;
    thread::sleep(Duration::from_secs(seconds));
    Ok(())
}

fn redraw_interval(page: Page) -> Duration {
    match page {
        Page::Clock
        | Page::SystemInfo
        | Page::MetricIp
        | Page::MetricCpuLoad
        | Page::MetricMemory
        | Page::MetricDisk
        | Page::MetricTemp => Duration::from_secs(1),
        Page::Logo
        | Page::PowerMenuCancel
        | Page::PowerMenuReboot
        | Page::PowerMenuShutdown
        | Page::Rebooting
        | Page::ShuttingDown => Duration::from_millis(200),
    }
}

fn transition_page(page: Page, button: ButtonEvent) -> Page {
    if matches!(page, Page::ShuttingDown | Page::Rebooting) {
        return page;
    }

    match button {
        ButtonEvent::K1 => match page {
            Page::PowerMenuCancel => Page::PowerMenuReboot,
            Page::PowerMenuReboot => Page::PowerMenuShutdown,
            Page::PowerMenuShutdown => Page::PowerMenuCancel,
            Page::Clock => Page::Logo,
            Page::Logo => Page::Clock,
            _ => Page::Clock,
        },
        ButtonEvent::K2 => match page {
            Page::PowerMenuCancel => Page::Clock,
            Page::PowerMenuReboot => Page::Rebooting,
            Page::PowerMenuShutdown => Page::ShuttingDown,
            Page::SystemInfo => Page::MetricIp,
            Page::MetricIp => Page::MetricCpuLoad,
            Page::MetricCpuLoad => Page::MetricMemory,
            Page::MetricMemory => Page::MetricDisk,
            Page::MetricDisk => Page::MetricTemp,
            Page::MetricTemp => Page::SystemInfo,
            _ => Page::SystemInfo,
        },
        ButtonEvent::K3 => match page {
            Page::PowerMenuCancel | Page::PowerMenuReboot | Page::PowerMenuShutdown => Page::Clock,
            _ => Page::PowerMenuCancel,
        },
    }
}

fn render_page(
    page: Page,
    framebuffer: &mut FrameBuffer,
    fonts: &FontSet,
    logo_image: &GrayImage,
    version_codename: Option<&str>,
) {
    framebuffer.clear();
    let metrics = if matches!(
        page,
        Page::SystemInfo
            | Page::MetricIp
            | Page::MetricCpuLoad
            | Page::MetricMemory
            | Page::MetricDisk
            | Page::MetricTemp
    ) {
        Some(SystemMetrics::gather())
    } else {
        None
    };

    match page {
        Page::Logo => render_logo_page(framebuffer, fonts, logo_image, version_codename),
        Page::Clock => render_clock_page(framebuffer, fonts),
        Page::SystemInfo => render_system_page(framebuffer, fonts, metrics.as_ref()),
        Page::MetricIp => render_metric_page(
            framebuffer,
            fonts,
            "IP",
            &metrics.as_ref().map(|m| m.ip.as_str()).unwrap_or("N/A"),
            true,
        ),
        Page::MetricCpuLoad => render_metric_page(
            framebuffer,
            fonts,
            "CPU Load",
            &metrics
                .as_ref()
                .map(|m| m.cpu_load.as_str())
                .unwrap_or("N/A"),
            false,
        ),
        Page::MetricMemory => render_metric_page(
            framebuffer,
            fonts,
            "Memory",
            &metrics
                .as_ref()
                .map(|m| extract_focus_value(&m.mem_usage))
                .unwrap_or_else(|| "N/A".to_string()),
            false,
        ),
        Page::MetricDisk => render_metric_page(
            framebuffer,
            fonts,
            "Disk",
            &metrics
                .as_ref()
                .map(|m| extract_focus_value(&m.disk_usage))
                .unwrap_or_else(|| "N/A".to_string()),
            false,
        ),
        Page::MetricTemp => render_metric_page(
            framebuffer,
            fonts,
            "CPU Temp",
            &metrics
                .as_ref()
                .map(|m| m.cpu_temp.as_str())
                .unwrap_or("N/A"),
            false,
        ),
        Page::PowerMenuCancel => render_power_menu(framebuffer, fonts, 0),
        Page::PowerMenuReboot => render_power_menu(framebuffer, fonts, 1),
        Page::PowerMenuShutdown => render_power_menu(framebuffer, fonts, 2),
        Page::Rebooting => render_rebooting_page(framebuffer, fonts),
        Page::ShuttingDown => render_shutting_down_page(framebuffer, fonts),
    }
}

fn render_logo_page(
    framebuffer: &mut FrameBuffer,
    fonts: &FontSet,
    logo_image: &GrayImage,
    version_codename: Option<&str>,
) {
    framebuffer.blit_luma_image(logo_image, 127);
    let Some(codename) = version_codename.filter(|v| !v.is_empty()) else {
        return;
    };

    let font_size = 10.0;
    let (w, h) = fonts.measure_text(codename, font_size, FontFace::Regular);
    let x = (127 - w).max(0);
    let y = (63 - h).max(0);

    let x0 = (x - 1).max(0);
    let y0 = (y - 1).max(0);
    let x1 = (x + w + 1).min(127);
    let y1 = (y + h + 1).min(63);
    framebuffer.fill_rect(x0, y0, x1 - x0 + 1, y1 - y0 + 1, false);
    fonts.draw_text(
        framebuffer,
        x,
        y,
        codename,
        font_size,
        FontFace::Regular,
        true,
    );
}

fn read_version_codename() -> Option<String> {
    let contents = fs::read_to_string("/etc/os-release").ok()?;
    parse_os_release_key(&contents, "VERSION_CODENAME")
        .or_else(|| parse_os_release_key(&contents, "DEBIAN_CODENAME"))
        .map(|v| capitalize_first(&v))
}

fn parse_os_release_key(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for line in contents.lines() {
        if let Some(raw) = line.strip_prefix(&prefix) {
            let value = raw.trim().trim_matches('"').trim_matches('\'').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn render_clock_page(framebuffer: &mut FrameBuffer, fonts: &FontSet) {
    let now = Local::now();
    let date = now.format("%a %e %b %Y").to_string();
    let progress = year_progress_percent(now);
    let bar_count = ((progress / 10.0).round() as i32).clamp(0, 10) as usize;
    let progress_line = format!(
        "{}{}{progress:.1}%",
        "▓".repeat(bar_count),
        "░".repeat(10 - bar_count)
    );
    let time = now.format("%T").to_string();

    fonts.draw_text(framebuffer, 2, 2, &date, 14.0, FontFace::Regular, true);
    fonts.draw_text(
        framebuffer,
        2,
        20,
        &progress_line,
        14.0,
        FontFace::Regular,
        true,
    );
    fonts.draw_text(framebuffer, 8, 38, &time, 24.0, FontFace::Bold, true);
}

fn render_system_page(
    framebuffer: &mut FrameBuffer,
    fonts: &FontSet,
    metrics: Option<&SystemMetrics>,
) {
    let fallback = SystemMetrics {
        ip: "N/A".to_string(),
        cpu_load: "N/A".to_string(),
        mem_usage: "N/A".to_string(),
        disk_usage: "N/A".to_string(),
        cpu_temp: "N/A".to_string(),
    };
    let metrics = metrics.unwrap_or(&fallback);
    let lines = [
        format!("IP: {}", metrics.ip),
        format!("CPU Load: {}", metrics.cpu_load),
        format!("Mem: {}", metrics.mem_usage),
        format!("Disk: {}", metrics.disk_usage),
        format!("CPU TEMP: {}", metrics.cpu_temp),
    ];

    for (idx, line) in lines.into_iter().enumerate() {
        fonts.draw_text(
            framebuffer,
            2,
            2 + idx as i32 * 12,
            &line,
            11.0,
            FontFace::Regular,
            true,
        );
    }
}

fn render_metric_page(
    framebuffer: &mut FrameBuffer,
    fonts: &FontSet,
    label: &str,
    value: &str,
    is_ip_page: bool,
) {
    let label_font_size = if is_ip_page { 12.0 } else { 16.0 };
    fonts.draw_text(
        framebuffer,
        2,
        0,
        label,
        label_font_size,
        FontFace::Regular,
        true,
    );

    let mut value_font_size = if is_ip_page { 18.0 } else { 36.0 };
    if is_ip_page {
        while value_font_size > 10.0 {
            let (w, _) = fonts.measure_text(value, value_font_size, FontFace::Bold);
            if w <= 124 {
                break;
            }
            value_font_size -= 1.0;
        }
    }

    let (w, h) = fonts.measure_text(value, value_font_size, FontFace::Bold);
    let content_top = if is_ip_page { 12 } else { 16 };
    let x = ((128 - w).max(0)) / 2;
    let y = content_top + ((64 - content_top - h).max(0)) / 2;
    fonts.draw_text(
        framebuffer,
        x,
        y,
        value,
        value_font_size,
        FontFace::Bold,
        true,
    );
}

fn extract_focus_value(raw: &str) -> String {
    raw.split_whitespace()
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

fn render_power_menu(framebuffer: &mut FrameBuffer, fonts: &FontSet, selected_index: usize) {
    fonts.draw_text(
        framebuffer,
        2,
        0,
        "Power Action",
        14.0,
        FontFace::Bold,
        true,
    );
    draw_option_row(framebuffer, fonts, 16, "Cancel", selected_index == 0);
    draw_option_row(framebuffer, fonts, 32, "Reboot", selected_index == 1);
    draw_option_row(framebuffer, fonts, 48, "Shutdown", selected_index == 2);
}

fn draw_option_row(
    framebuffer: &mut FrameBuffer,
    fonts: &FontSet,
    y: i32,
    text: &str,
    selected: bool,
) {
    let x = 2;
    let w = 123;
    let h = 14;

    framebuffer.fill_rect(x, y, w, h, selected);
    framebuffer.draw_rect(x, y, w, h, true);
    fonts.draw_text(
        framebuffer,
        4,
        y + 1,
        text,
        11.0,
        FontFace::Regular,
        !selected,
    );
}

fn render_rebooting_page(framebuffer: &mut FrameBuffer, fonts: &FontSet) {
    fonts.draw_text(framebuffer, 2, 2, "Rebooting", 14.0, FontFace::Bold, true);
    fonts.draw_text(
        framebuffer,
        2,
        20,
        "Please wait",
        11.0,
        FontFace::Regular,
        true,
    );
}

fn render_shutting_down_page(framebuffer: &mut FrameBuffer, fonts: &FontSet) {
    fonts.draw_text(
        framebuffer,
        2,
        2,
        "Shutting down",
        14.0,
        FontFace::Bold,
        true,
    );
    fonts.draw_text(
        framebuffer,
        2,
        20,
        "Please wait",
        11.0,
        FontFace::Regular,
        true,
    );
}

fn year_progress_percent(now: chrono::DateTime<Local>) -> f32 {
    let start = Local
        .with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    let end = Local
        .with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(now);

    let elapsed = (now.timestamp_millis() - start.timestamp_millis()) as f64;
    let total = (end.timestamp_millis() - start.timestamp_millis()) as f64;
    if total <= 0.0 {
        return 0.0;
    }
    (elapsed * 100.0 / total).clamp(0.0, 100.0) as f32
}

fn run_poweroff(dry_run: bool) -> Result<()> {
    if dry_run {
        warn!("dry-run enabled, skipping poweroff");
        return Ok(());
    }

    let status = Command::new("systemctl")
        .arg("poweroff")
        .status()
        .context("failed to execute systemctl poweroff")?;
    if status.success() {
        return Ok(());
    }

    error!("systemctl poweroff exited with {status}, fallback to shutdown -h now");
    let status = Command::new("shutdown")
        .args(["-h", "now"])
        .status()
        .context("failed to execute shutdown -h now")?;
    if status.success() {
        Ok(())
    } else {
        bail!("shutdown -h now exited with {status}")
    }
}

fn run_reboot(dry_run: bool) -> Result<()> {
    if dry_run {
        warn!("dry-run enabled, skipping reboot");
        return Ok(());
    }

    let status = Command::new("systemctl")
        .arg("reboot")
        .status()
        .context("failed to execute systemctl reboot")?;
    if status.success() {
        return Ok(());
    }

    error!("systemctl reboot exited with {status}, fallback to reboot");
    let status = Command::new("reboot")
        .status()
        .context("failed to execute reboot")?;
    if status.success() {
        Ok(())
    } else {
        bail!("reboot exited with {status}")
    }
}

fn parse_u8_auto(raw: &str) -> Result<u8, String> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16).map_err(|e| format!("invalid hex u8 `{raw}`: {e}"))
    } else {
        raw.parse::<u8>()
            .map_err(|e| format!("invalid u8 `{raw}`: {e}"))
    }
}
