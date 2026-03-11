use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use nix::sys::statvfs::statvfs;

#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub ip: String,
    pub cpu_load: String,
    pub mem_usage: String,
    pub disk_usage: String,
    pub cpu_temp: String,
}

impl SystemMetrics {
    pub fn gather() -> Self {
        Self {
            ip: format_ip(),
            cpu_load: format_cpu_load().unwrap_or_else(|_| "N/A".to_string()),
            mem_usage: format_mem_usage().unwrap_or_else(|_| "N/A".to_string()),
            disk_usage: format_disk_usage().unwrap_or_else(|_| "N/A".to_string()),
            cpu_temp: format_cpu_temp().unwrap_or_else(|_| "N/A".to_string()),
        }
    }
}

fn format_ip() -> String {
    if let Some(ip) = eth0_ipv4() {
        return ip;
    }

    let fallback = "127.0.0.1".to_string();
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return fallback,
    };

    let target: SocketAddr = match "10.255.255.255:1".parse() {
        Ok(v) => v,
        Err(_) => return fallback,
    };

    let _ = socket.connect(target);
    let ip = socket
        .local_addr()
        .ok()
        .map(|addr| addr.ip().to_string())
        .unwrap_or(fallback);
    ip
}

fn eth0_ipv4() -> Option<String> {
    let output = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", "eth0"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let cidr = stdout.split_whitespace().nth(3)?;
    Some(cidr.split('/').next()?.to_string())
}

fn format_cpu_load() -> Result<String> {
    let loadavg = fs::read_to_string("/proc/loadavg").context("read /proc/loadavg failed")?;
    let load = loadavg
        .split_whitespace()
        .next()
        .context("missing loadavg first column")?
        .parse::<f32>()
        .context("invalid loadavg value")?;
    Ok(format!("{load:.2}"))
}

fn format_mem_usage() -> Result<String> {
    let meminfo = fs::read_to_string("/proc/meminfo").context("read /proc/meminfo failed")?;
    let mut total_kb = None::<u64>;
    let mut avail_kb = None::<u64>;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = parse_meminfo_kb(line);
        } else if line.starts_with("MemAvailable:") {
            avail_kb = parse_meminfo_kb(line);
        }
    }

    let total_kb = total_kb.context("MemTotal missing")?;
    let avail_kb = avail_kb.context("MemAvailable missing")?;
    let used_kb = total_kb.saturating_sub(avail_kb);
    let total_mb = total_kb / 1024;
    let used_mb = used_kb / 1024;
    let percent = if total_kb == 0 {
        0.0
    } else {
        used_kb as f64 * 100.0 / total_kb as f64
    };

    Ok(format!("{used_mb}/{total_mb}MB {percent:.1}%"))
}

fn parse_meminfo_kb(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

fn format_disk_usage() -> Result<String> {
    let stat = statvfs(Path::new("/")).context("statvfs(/) failed")?;
    let block_size = stat.fragment_size();
    let total = stat.blocks() * block_size;
    let available = stat.blocks_available() * block_size;
    let used = total.saturating_sub(available);

    let gib = 1024f64 * 1024f64 * 1024f64;
    let total_gb = (total as f64 / gib).round() as u64;
    let used_gb = (used as f64 / gib).round() as u64;
    let percent = if total == 0 {
        0
    } else {
        ((used as f64 * 100.0 / total as f64).round()) as u64
    };

    Ok(format!("{used_gb}/{total_gb}GB {percent}%"))
}

fn format_cpu_temp() -> Result<String> {
    let temp_path = Path::new("/sys/class/thermal/thermal_zone0/temp");
    let raw = fs::read_to_string(temp_path).context("read thermal zone failed")?;
    let val = raw.trim().parse::<f64>().context("parse cpu temp failed")?;
    let celsius = if val > 1000.0 { val / 1000.0 } else { val };

    if (celsius.fract()).abs() < f64::EPSILON {
        Ok(format!("{}°C", celsius as i64))
    } else {
        Ok(format!("{celsius:.1}°C"))
    }
}
