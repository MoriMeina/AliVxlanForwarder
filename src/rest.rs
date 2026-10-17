use axum::{http::StatusCode, routing::get, Extension, Json, Router, Server};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::stats::Stats;

#[derive(Serialize)]
struct StatsResponse {
    rx_bytes: u64,
    tx_bytes: u64,
    drop_count: u64,
    drops_vni_filtered: u64,
    drops_parse_error: u64,
    drops_send_failed: u64,
    drops_too_short: u64,
    rx_bps: f64,
    tx_bps: f64,
}

async fn stats_handler(Extension(stats): Extension<Arc<Stats>>) -> Result<Json<StatsResponse>, StatusCode> {
    let (rx_bps, tx_bps) = stats.get_smoothed_bps();
    Ok(Json(StatsResponse {
        rx_bytes: stats.rx_bytes.load(std::sync::atomic::Ordering::Relaxed),
        tx_bytes: stats.tx_bytes.load(std::sync::atomic::Ordering::Relaxed),
        drop_count: stats.get_total_drops(),
        drops_vni_filtered: stats.drops_vni_filtered.load(std::sync::atomic::Ordering::Relaxed),
        drops_parse_error: stats.drops_parse_error.load(std::sync::atomic::Ordering::Relaxed),
        drops_send_failed: stats.drops_send_failed.load(std::sync::atomic::Ordering::Relaxed),
        drops_too_short: stats.drops_too_short.load(std::sync::atomic::Ordering::Relaxed),
        rx_bps,
        tx_bps,
    }))
}

async fn metrics_handler(Extension(stats): Extension<Arc<Stats>>) -> String {
    let (rx_bps, tx_bps) = stats.get_smoothed_bps();
    let rx = stats.rx_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let tx = stats.tx_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let d_vni = stats.drops_vni_filtered.load(std::sync::atomic::Ordering::Relaxed);
    let d_parse = stats.drops_parse_error.load(std::sync::atomic::Ordering::Relaxed);
    let d_send = stats.drops_send_failed.load(std::sync::atomic::Ordering::Relaxed);
    let d_short = stats.drops_too_short.load(std::sync::atomic::Ordering::Relaxed);

    format!(
        "# HELP alivxlan_rx_bytes_total Total received bytes\n\
         # TYPE alivxlan_rx_bytes_total counter\n\
         alivxlan_rx_bytes_total {rx}\n\
         # HELP alivxlan_tx_bytes_total Total transmitted bytes\n\
         # TYPE alivxlan_tx_bytes_total counter\n\
         alivxlan_tx_bytes_total {tx}\n\
         # HELP alivxlan_rx_bps Smoothed receive rate (bps)\n\
         # TYPE alivxlan_rx_bps gauge\n\
         alivxlan_rx_bps {rx_bps}\n\
         # HELP alivxlan_tx_bps Smoothed transmit rate (bps)\n\
         # TYPE alivxlan_tx_bps gauge\n\
         alivxlan_tx_bps {tx_bps}\n\
         # HELP alivxlan_drops_total Total dropped packets\n\
         # TYPE alivxlan_drops_total counter\n\
         alivxlan_drops_total {}\n\
         # HELP alivxlan_drops_vni_filtered Packets dropped by VNI filter\n\
         # TYPE alivxlan_drops_vni_filtered counter\n\
         alivxlan_drops_vni_filtered {d_vni}\n\
         # HELP alivxlan_drops_parse_error Packets dropped by parse error\n\
         # TYPE alivxlan_drops_parse_error counter\n\
         alivxlan_drops_parse_error {d_parse}\n\
         # HELP alivxlan_drops_send_failed Packets dropped by send failure\n\
         # TYPE alivxlan_drops_send_failed counter\n\
         alivxlan_drops_send_failed {d_send}\n\
         # HELP alivxlan_drops_too_short Packets dropped (too short)\n\
         # TYPE alivxlan_drops_too_short counter\n\
         alivxlan_drops_too_short {d_short}\n",
        d_vni + d_parse + d_send + d_short
    )
}

pub async fn serve(stats: Arc<Stats>, addr: SocketAddr) {
    let app = Router::new()
        .route("/stats", get(stats_handler))
        .route("/metrics", get(metrics_handler))
        .layer(Extension(stats));

    tracing::info!("REST API 监听 {}", addr);

    if let Err(e) = Server::bind(&addr).serve(app.into_make_service()).await {
        tracing::error!("REST API 服务异常: {}", e);
    }
}
