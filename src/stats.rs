use std::collections::VecDeque;
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
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
    alpha: f64,
    last_smoothed_rx_bps: Mutex<f64>,
    last_smoothed_tx_bps: Mutex<f64>,
    pub rx_bytes: AtomicU64,
    pub tx_bytes: AtomicU64,
    pub drop_count: AtomicU64,
}

impl Stats {
    /// 累加接收字节数
    pub fn add_rx_bytes(&self, bytes: u64) {
        self.rx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// 累加发送字节数
    pub fn add_tx_bytes(&self, bytes: u64) {
        self.tx_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// 获取当前总接收字节数
    pub fn get_total_rx(&self) -> u64 {
        self.rx_bytes.load(Ordering::Relaxed)
    }

    /// 获取当前总发送字节数
    pub fn get_total_tx(&self) -> u64 {
        self.tx_bytes.load(Ordering::Relaxed)
    }
    pub fn new() -> Self {
        Self {
            history: Mutex::new(VecDeque::new()),
            window: Duration::from_secs(10),  // 平滑窗口，比如10秒
            alpha: 0.3,                      // 指数加权系数，0~1之间
            last_smoothed_rx_bps: Mutex::new(0.0),
            last_smoothed_tx_bps: Mutex::new(0.0),
            rx_bytes: AtomicU64::new(0),
            tx_bytes: AtomicU64::new(0),
            drop_count: AtomicU64::new(0),
        }
    }

    /// 定时调用，更新采样历史，传入当前时间和总的收发字节数快照
    pub fn update(&self, now: Instant, rx_bytes: u64, tx_bytes: u64) {
        let mut history = self.history.lock().unwrap();

        // 清理超过时间窗口的旧采样
        while let Some(front) = history.front() {
            if now.duration_since(front.time) > self.window {
                history.pop_front();
            } else {
                break;
            }
        }

        history.push_back(Sample { time: now, rx_bytes, tx_bytes });
    }

    /// 获取平滑后的速率（bps）
    pub fn get_smoothed_bps(&self) -> (f64, f64) {
        let history = self.history.lock().unwrap();

        if history.len() < 2 {
            return (
                *self.last_smoothed_rx_bps.lock().unwrap(),
                *self.last_smoothed_tx_bps.lock().unwrap(),
            );
        }

        let mut weighted_rx_sum = 0.0;
        let mut weighted_tx_sum = 0.0;
        let mut weight_total = 0.0;

        let samples = history.as_slices().0;

        for pair in samples.windows(2) {
            let older = &pair[0];
            let newer = &pair[1];
            let dt = newer.time.duration_since(older.time).as_secs_f64();

            if dt <= 0.0 {
                continue;
            }

            let rx_diff = newer.rx_bytes.saturating_sub(older.rx_bytes) as f64;
            let tx_diff = newer.tx_bytes.saturating_sub(older.tx_bytes) as f64;

            let rx_bps = rx_diff * 8.0 / dt;
            let tx_bps = tx_diff * 8.0 / dt;

            weighted_rx_sum += rx_bps * dt;
            weighted_tx_sum += tx_bps * dt;
            weight_total += dt;
        }

        if weight_total == 0.0 {
            return (
                *self.last_smoothed_rx_bps.lock().unwrap(),
                *self.last_smoothed_tx_bps.lock().unwrap(),
            );
        }

        let rx_rate = weighted_rx_sum / weight_total;
        let tx_rate = weighted_tx_sum / weight_total;

        {
            let mut rx_lock = self.last_smoothed_rx_bps.lock().unwrap();
            *rx_lock = self.alpha * rx_rate + (1.0 - self.alpha) * *rx_lock;
        }
        {
            let mut tx_lock = self.last_smoothed_tx_bps.lock().unwrap();
            *tx_lock = self.alpha * tx_rate + (1.0 - self.alpha) * *tx_lock;
        }

        (
            *self.last_smoothed_rx_bps.lock().unwrap(),
            *self.last_smoothed_tx_bps.lock().unwrap(),
        )
    }
}
