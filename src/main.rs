mod stats;
mod forwarder;
mod tap;
mod rest;
mod args;

use clap::Parser;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use std::time::{Duration, Instant};

use crate::args::Args;
use crate::forwarder::{TapForwarder, RawForwarder, Forwarder};
use crate::stats::Stats;
use crate::tap::TapInterface;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    args.validate();

    let stats = Arc::new(Stats::new());

    let (_tx, _rx) = channel::<Vec<u8>>(4096);

    let forwarder: Arc<dyn Forwarder>;
    let _tap_keep_alive;

    if let Some(tn) = &args.tap {
        let tap = TapInterface::create(tn).expect("创建 TAP 失败");
        forwarder = Arc::new(TapForwarder::new(tap.fd(), stats.clone())); // 传递 stats 用于统计
        _tap_keep_alive = Some(tap);
    } else if let Some(o) = &args.output {
        forwarder = Arc::new(RawForwarder::new(o, stats.clone()));
        _tap_keep_alive = None;
    } else {
        unreachable!("必须指定 --tap 或 --output 参数");
    }

    // 启动 REST API 服务
    {
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            rest::serve(stats_clone).await;
        });
    }

    // 定时更新采样并打印平滑速率
    {
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let now = Instant::now();
                let rx = stats_clone.get_total_rx();
                let tx = stats_clone.get_total_tx();
                stats_clone.update(now, rx, tx);

                let (rx_bps, tx_bps) = stats_clone.get_smoothed_bps();
                println!("平滑速率: rx = {:.2} bps, tx = {:.2} bps", rx_bps, tx_bps);
            }
        });
    }

    forwarder::run_forwarder(&args, forwarder, stats).await;
}
