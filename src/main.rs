mod args;
mod forwarder;
mod rest;
mod stats;
mod tap;

use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::signal::ctrl_c;

use crate::args::Args;
use crate::forwarder::{PcapForwarder, RawForwarder, Forwarder, TapForwarder};
use crate::stats::Stats;
use crate::tap::TapInterface;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Err(e) = args.validate() {
        eprintln!("参数错误: {e}");
        std::process::exit(1);
    }

    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level)),
        )
        .init();

    tracing::info!("AliVxlan Forwarder 启动");
    tracing::info!("输入接口: {}", args.input);
    if let Some(ref tap) = args.tap {
        tracing::info!("输出模式: TAP {}", tap);
    } else if let Some(ref out) = args.output {
        tracing::info!("输出模式: Raw {}", out);
    } else if let Some(ref pcap) = args.pcap {
        tracing::info!("输出模式: PCAP {}", pcap);
    }
    if !args.vni.is_empty() {
        tracing::info!("VNI 过滤: {:?}", args.vni);
    } else {
        tracing::info!("VNI 过滤: 无 (全部通过)");
    }

    let stats = Arc::new(Stats::new());
    let shutdown = Arc::new(AtomicBool::new(false));

    // 创建转发器
    let forwarder: Arc<dyn Forwarder>;
    let mut _tap_keep_alive: Option<TapInterface> = None;

    if let Some(tn) = &args.tap {
        let mut tap = TapInterface::create(tn).expect("创建 TAP 失败");
        let file = tap.take_file();
        forwarder = Arc::new(TapForwarder::new(file, stats.clone()));
        _tap_keep_alive = Some(tap);
    } else if let Some(o) = &args.output {
        forwarder = Arc::new(RawForwarder::new(o, stats.clone()));
    } else if let Some(p) = &args.pcap {
        forwarder = Arc::new(
            PcapForwarder::create(p, stats.clone()).expect("创建 PCAP 文件失败"),
        );
    } else {
        unreachable!("validate 已保证至少指定一个输出");
    }

    // 启动 REST API
    let api_addr: std::net::SocketAddr = args
        .api_addr
        .parse()
        .expect("api_addr 格式无效");
    {
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            rest::serve(stats_clone, api_addr).await;
        });
    }

    // 定时采样并打印平滑速率
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
                tracing::info!("平滑速率: rx = {:.2} bps, tx = {:.2} bps", rx_bps, tx_bps);
            }
        });
    }

    // Ctrl+C 信号处理（也由转发循环内的 select 处理，这里做兜底）
    {
        let shutdown_clone = shutdown.clone();
        tokio::spawn(async move {
            if ctrl_c().await.is_ok() {
                tracing::info!("收到 Ctrl+C 信号");
                shutdown_clone.store(true, Ordering::Relaxed);
            }
        });
    }

    // 主转发循环
    forwarder::run_forwarder(&args, &forwarder, stats, shutdown).await;

    tracing::info!("AliVxlan Forwarder 已停止");
}
