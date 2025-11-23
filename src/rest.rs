use axum::{
    Router,
    routing::get,
    http::StatusCode,
    Json,
    Extension,
    Server,
};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::stats::Stats;

#[derive(Serialize)]
struct StatsResponse {
    rx_bytes: u64,
    tx_bytes: u64,
    drop_count: u64,
    rx_bps: f64,
    tx_bps: f64,
}

async fn stats_handler(Extension(stats): Extension<Arc<Stats>>) -> Result<Json<StatsResponse>, StatusCode> {
    let rx = stats.rx_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let tx = stats.tx_bytes.load(std::sync::atomic::Ordering::Relaxed);
    let drop = stats.drop_count.load(std::sync::atomic::Ordering::Relaxed);
    let (rx_bps, tx_bps) = stats.get_smoothed_bps();

    Ok(Json(StatsResponse {
        rx_bytes: rx,
        tx_bytes: tx,
        drop_count: drop,
        rx_bps,
        tx_bps,
    }))
}

pub async fn serve(stats: Arc<Stats>) {
    let app = Router::new()
        .route("/stats", get(stats_handler))
        .layer(Extension(stats));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("[*] REST API listening on {}", addr);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
