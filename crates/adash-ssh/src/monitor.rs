use crate::Connection;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

const PROBE: &str = "printf '__STAT__\\n'; head -n 1 /proc/stat; printf '__MEM__\\n'; cat /proc/meminfo; printf '__NET__\\n'; cat /proc/net/dev; printf '__UP__\\n'; cat /proc/uptime; printf '__LOAD__\\n'; cat /proc/loadavg; printf '__DISK__\\n'; df -Pk /";
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorSample {
    pub cpu_total: u64,
    pub cpu_idle: u64,
    pub memory_total: u64,
    pub memory_available: u64,
    pub network_received: u64,
    pub network_sent: u64,
    pub uptime_seconds: f64,
    pub load: String,
    pub disk_total_kib: u64,
    pub disk_used_kib: u64,
}
impl MonitorSample {
    pub fn cpu_percent(&self, previous: &Self) -> Option<f64> {
        let total = self.cpu_total.checked_sub(previous.cpu_total)?;
        let idle = self.cpu_idle.checked_sub(previous.cpu_idle)?;
        (total > 0)
            .then(|| (100. * (total.saturating_sub(idle)) as f64 / total as f64).clamp(0., 100.))
    }
    pub fn memory_percent(&self) -> f64 {
        if self.memory_total == 0 {
            return 0.;
        }
        100. * self.memory_total.saturating_sub(self.memory_available) as f64
            / self.memory_total as f64
    }
}
impl Connection {
    pub fn monitor(&self) -> Result<MonitorSample> {
        let result = self.execute(PROBE)?;
        if result.exit_status != 0 {
            bail!("监控采集失败；需要 Linux /proc 与 df");
        }
        parse_monitor(&result.stdout)
    }
}
pub fn parse_monitor(text: &str) -> Result<MonitorSample> {
    let mut result = MonitorSample::default();
    let mut section = "";
    for line in text.lines() {
        if line.starts_with("__") {
            section = line;
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let number = |index: usize| {
            parts
                .get(index)
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
        };
        match section {
            "__STAT__" if parts.first() == Some(&"cpu") => {
                // guest counters are already included in user/nice.
                result.cpu_total = parts
                    .iter()
                    .skip(1)
                    .take(8)
                    .filter_map(|value| value.parse::<u64>().ok())
                    .sum();
                result.cpu_idle = number(4) + number(5);
            }
            "__MEM__" => match parts.first().copied() {
                Some("MemTotal:") => result.memory_total = number(1) * 1024,
                Some("MemAvailable:") => result.memory_available = number(1) * 1024,
                _ => {}
            },
            "__NET__" if line.contains(':') => {
                let (device, values) = line.split_once(':').unwrap();
                if device.trim() != "lo" {
                    let values: Vec<_> = values.split_whitespace().collect();
                    result.network_received += values
                        .first()
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    result.network_sent += values
                        .get(8)
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                }
            }
            "__UP__" => {
                result.uptime_seconds = parts
                    .first()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.)
            }
            "__LOAD__" => {
                result.load = parts
                    .iter()
                    .take(3)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" / ")
            }
            "__DISK__" if parts.len() >= 6 && parts[1].parse::<u64>().is_ok() => {
                result.disk_total_kib = number(1);
                result.disk_used_kib = number(2);
            }
            _ => {}
        }
    }
    if result.memory_total == 0 || result.cpu_total == 0 {
        bail!("远程系统未返回有效的 Linux 监控数据");
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_uses_deltas_and_does_not_double_count_guests() {
        let text = "__STAT__\ncpu 100 20 30 400 50 1 2 3 70 10\n__MEM__\nMemTotal: 1000 kB\nMemAvailable: 250 kB\n__NET__\neth0: 120 0 0 0 0 0 0 0 300 0\n__UP__\n100.5 0\n__DISK__\n/dev/root 100 25 75 25% /\n";
        let sample = parse_monitor(text).unwrap();
        assert_eq!(sample.cpu_total, 606);
        assert_eq!(sample.memory_percent(), 75.);
        assert_eq!(sample.network_sent, 300);
        let next = MonitorSample {
            cpu_total: 706,
            cpu_idle: 500,
            ..sample.clone()
        };
        assert_eq!(next.cpu_percent(&sample), Some(50.));
    }
}
