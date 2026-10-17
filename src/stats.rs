use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Sample {
    time: Instant,
    rx_bytes: u64,
    tx_bytes: u64,
}

pub struct Stats {
    history: Mutex<VecDeque<Sample>>,
    window: Duration,
    pub rx_bytes: AtomicU64,
    pub tx_bytes: AtomicU64,
    pub drops_vni_filtered: AtomicU64,
    pub drops_parse_error: AtomicU64,
    pub drops_send_failed: AtomicU64,
    pub drops_too_short: AtomicU64,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            history: Mutex::new(VecDeque::new()),
            window: Duration::from_secs(10),
            rx_bytes: AtomicU64::new(0),
            tx_bytes: AtomicU64::new(0),
            drops_vni_filtered: AtomicU64::new(0),
            drops_parse_error: AtomicU64::new(0),
            drops_send_failed: AtomicU64::new(0),
            drops_too_short: AtomicU64::new(0),
        }
    }

    pub fn add_rx_bytes(&self, bytes: u64) {
        self.rx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_tx_bytes(&self, bytes: u64) {
        self.tx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_total_rx(&self) -> u64 {
        self.rx_bytes.load(Ordering::Relaxed)
    }

    pub fn get_total_tx(&self) -> u64 {
        self.tx_bytes.load(Ordering::Relaxed)
    }

    pub fn get_total_drops(&self) -> u64 {
        self.drops_vni_filtered.load(Ordering::Relaxed)
            + self.drops_parse_error.load(Ordering::Relaxed)
            + self.drops_send_failed.load(Ordering::Relaxed)
            + self.drops_too_short.load(Ordering::Relaxed)
    }

    /// 定时调用，记录采样
    pub fn update(&self, now: Instant, rx_bytes: u64, tx_bytes: u64) {
        let mut history = self.history.lock().unwrap();

        while let Some(front) = history.front() {
            if now.duration_since(front.time) > self.window {
                history.pop_front();
            } else {
                break;
            }
        }

        history.push_back(Sample {
            time: now,
            rx_bytes,
            tx_bytes,
        });
    }

    /// 计算窗口内时间加权平均速率（bps）。纯函数，无副作用。
    pub fn get_smoothed_bps(&self) -> (f64, f64) {
        let history = self.history.lock().unwrap();

        if history.len() < 2 {
            return (0.0, 0.0);
        }

        let mut weighted_rx_sum = 0.0;
        let mut weighted_tx_sum = 0.0;
        let mut weight_total = 0.0;

        let mut prev: Option<&Sample> = None;
        for cur in history.iter() {
            if let Some(prev) = prev {
                let dt = cur.time.duration_since(prev.time).as_secs_f64();
                if dt > 0.0 {
                    let rx_diff = cur.rx_bytes.saturating_sub(prev.rx_bytes) as f64;
                    let tx_diff = cur.tx_bytes.saturating_sub(prev.tx_bytes) as f64;
                    let rx_bps = rx_diff * 8.0 / dt;
                    let tx_bps = tx_diff * 8.0 / dt;
                    weighted_rx_sum += rx_bps * dt;
                    weighted_tx_sum += tx_bps * dt;
                    weight_total += dt;
                }
            }
            prev = Some(cur);
        }

        if weight_total == 0.0 {
            return (0.0, 0.0);
        }

        (
            weighted_rx_sum / weight_total,
            weighted_tx_sum / weight_total,
        )
    }
}
