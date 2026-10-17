use async_trait::async_trait;
use pnet::datalink::{self, Channel::Ethernet};
use pnet::packet::{
    ethernet::EthernetPacket, ip::IpNextHeaderProtocols, ipv4::Ipv4Packet, udp::UdpPacket, Packet,
};
use std::collections::HashSet;
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::stats::Stats;

/// 确保网络接口为 UP 状态
fn ensure_interface_up(name: &str) {
    let iface = datalink::interfaces()
        .into_iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("找不到接口 {}", name));

    if !iface.is_up() {
        tracing::info!("接口 {} 处于 DOWN 状态，尝试设置为 UP...", name);
        let status = Command::new("ip")
            .args(["link", "set", "dev", name, "up"])
            .status()
            .expect("无法执行 ip 命令");

        if !status.success() {
            panic!("设置接口 {} 为 UP 失败", name);
        }
    }
}

#[async_trait]
pub trait Forwarder: Send + Sync {
    async fn send(&self, frame: Vec<u8>) -> std::io::Result<()>;
}

// ---------------------------------------------------------------------------
// TAP 转发器
// ---------------------------------------------------------------------------

pub struct TapForwarder {
    file: Mutex<tokio::fs::File>,
    stats: Arc<Stats>,
}

impl TapForwarder {
    pub fn new(file: std::fs::File, stats: Arc<Stats>) -> Self {
        let file = tokio::fs::File::from_std(file);
        Self {
            file: Mutex::new(file),
            stats,
        }
    }
}

#[async_trait]
impl Forwarder for TapForwarder {
    async fn send(&self, frame: Vec<u8>) -> std::io::Result<()> {
        let mut f = self.file.lock().await;
        f.write_all(&frame).await?;
        self.stats.add_tx_bytes(frame.len() as u64);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Raw 网卡转发器
// ---------------------------------------------------------------------------

pub struct RawForwarder {
    sender: std::sync::Mutex<Box<dyn datalink::DataLinkSender>>,
    stats: Arc<Stats>,
}

impl RawForwarder {
    pub fn new(iface_name: &str, stats: Arc<Stats>) -> Self {
        ensure_interface_up(iface_name);

        let iface = datalink::interfaces()
            .into_iter()
            .find(|i| i.name == iface_name)
            .unwrap_or_else(|| panic!("找不到接口 {}", iface_name));

        let (tx, _rx) = match datalink::channel(&iface, Default::default())
            .unwrap_or_else(|e| panic!("创建 channel 失败: {}", e))
        {
            Ethernet(tx, rx) => (tx, rx),
            _ => panic!("不支持的通道类型"),
        };

        Self {
            sender: std::sync::Mutex::new(tx),
            stats,
        }
    }
}

#[async_trait]
impl Forwarder for RawForwarder {
    async fn send(&self, frame: Vec<u8>) -> std::io::Result<()> {
        let mut sender = self.sender.lock().unwrap();
        match sender.send_to(&frame, None) {
            Some(Ok(_len)) => {
                self.stats.add_tx_bytes(frame.len() as u64);
                Ok(())
            }
            Some(Err(e)) => {
                self.stats.drops_send_failed.fetch_add(1, Ordering::Relaxed);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("发送失败: {:?}", e),
                ))
            }
            None => {
                self.stats.drops_send_failed.fetch_add(1, Ordering::Relaxed);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "发送失败: None 返回",
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PCAP 文件转发器
// ---------------------------------------------------------------------------

pub struct PcapForwarder {
    file: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
    stats: Arc<Stats>,
    counter: AtomicU64,
}

impl PcapForwarder {
    pub fn create(path: &str, stats: Arc<Stats>) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);

        // PCAP 全局头 (24 字节, 小端序)
        let header: [u8; 24] = [
            0xd4, 0xc3, 0xb2, 0xa1, // magic
            0x02, 0x00, // version major
            0x04, 0x00, // version minor
            0x00, 0x00, 0x00, 0x00, // thiszone
            0x00, 0x00, 0x00, 0x00, // sigfigs
            0xff, 0xff, 0x00, 0x00, // snaplen (65535)
            0x01, 0x00, 0x00, 0x00, // network (Ethernet)
        ];
        writer.write_all(&header)?;
        writer.flush()?;

        tracing::info!("PCAP 文件已创建: {}", path);
        Ok(Self {
            file: std::sync::Mutex::new(writer),
            stats,
            counter: AtomicU64::new(0),
        })
    }
}

#[async_trait]
impl Forwarder for PcapForwarder {
    async fn send(&self, frame: Vec<u8>) -> std::io::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ts_sec = now.as_secs() as u32;
        let ts_usec = now.subsec_micros() as u32;
        let len = frame.len() as u32;

        let mut writer = self.file.lock().unwrap();
        writer.write_all(&ts_sec.to_le_bytes())?;
        writer.write_all(&ts_usec.to_le_bytes())?;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&len.to_le_bytes())?;
        writer.write_all(&frame)?;

        let n = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 1000 == 0 {
            writer.flush()?;
        }

        self.stats.add_tx_bytes(frame.len() as u64);
        Ok(())
    }
}

impl Drop for PcapForwarder {
    fn drop(&mut self) {
        if let Ok(mut writer) = self.file.lock() {
            let _ = writer.flush();
        }
    }
}

// ---------------------------------------------------------------------------
// 协议解析与过滤
// ---------------------------------------------------------------------------

/// 跳过 802.1Q VLAN 标签（支持 QinQ 嵌套），返回 (内层 EtherType, payload)
fn skip_vlan<'a>(eth: &'a EthernetPacket<'a>) -> (u16, &'a [u8]) {
    let mut ethertype = eth.get_ethertype().0;
    let mut payload: &[u8] = eth.payload();

    while ethertype == 0x8100 && payload.len() >= 4 {
        ethertype = u16::from_be_bytes([payload[2], payload[3]]);
        payload = &payload[4..];
    }

    (ethertype, payload)
}

/// 解析 AliVxlan 报文，返回需要转发的以太网帧。
/// 返回 None 表示该帧被过滤或解析失败。
fn parse_and_filter(frame: &[u8], vni_set: &Option<HashSet<u32>>, stats: &Stats) -> Option<Vec<u8>> {
    let eth = EthernetPacket::new(frame)?;

    let (ethertype, payload) = skip_vlan(&eth);

    if ethertype != 0x0800 {
        return None;
    }

    let ipv4 = Ipv4Packet::new(payload)?;
    if ipv4.get_next_level_protocol() != IpNextHeaderProtocols::Udp {
        return None;
    }

    let udp = UdpPacket::new(ipv4.payload())?;
    if udp.get_destination() != 250 {
        return None;
    }

    let p = udp.payload();
    if p.len() < 8 {
        stats.drops_too_short.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    // VXLAN 头: flags(1) | reserved(1) | VNI(3) | reserved(1)
    // VNI 在字节 [4..7)，byte 7 为 reserved
    let vni = ((p[4] as u32) << 16) | ((p[5] as u32) << 8) | (p[6] as u32);

    if let Some(vs) = vni_set {
        if !vs.contains(&vni) {
            stats.drops_vni_filtered.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    }

    let inner = &p[8..];
    let payload_type = (p[1] & 0x08) >> 3;

    match payload_type {
        0 if inner.len() >= 20 => {
            // IP over IP: 封装为完整以太网帧
            let mut buf = Vec::with_capacity(14 + inner.len());
            buf.extend_from_slice(&[0x02, 0, 0, 0, 0, 1]); // dst MAC
            buf.extend_from_slice(&[0x02, 0, 0, 0, 0, 2]); // src MAC
            buf.extend_from_slice(&[0x08, 0x00]); // EtherType: IPv4
            buf.extend_from_slice(inner);
            Some(buf)
        }
        1 if inner.len() >= 14 => {
            // MAC over IP: 原样输出完整二层帧
            Some(inner.to_vec())
        }
        _ => {
            stats.drops_parse_error.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 主转发循环
// ---------------------------------------------------------------------------

pub async fn run_forwarder(
    args: &crate::args::Args,
    fwd: &Arc<dyn Forwarder>,
    stats: Arc<Stats>,
    shutdown: Arc<AtomicBool>,
) {
    let fwd = fwd.clone();
    ensure_interface_up(&args.input);

    let iface = datalink::interfaces()
        .into_iter()
        .find(|i| i.name == args.input)
        .unwrap_or_else(|| panic!("找不到输入接口 {}", args.input));

    let (_tx, mut rx) = match datalink::channel(&iface, Default::default())
        .unwrap_or_else(|e| panic!("创建 channel 失败: {}", e))
    {
        Ethernet(tx, rx) => (tx, rx),
        _ => panic!("只支持 Ethernet 通道"),
    };

    let vni_set = if args.vni.is_empty() {
        None
    } else {
        Some(args.vni.iter().copied().collect::<HashSet<_>>())
    };

    let (frame_tx, mut frame_rx) = mpsc::channel::<Vec<u8>>(4096);

    // 阻塞收包线程：pnet 的 rx.next() 是同步阻塞调用，
    // 放在独立 OS 线程中避免阻塞 tokio 运行时。
    let stats_recv = stats.clone();
    let shutdown_recv = shutdown.clone();
    std::thread::spawn(move || {
        tracing::info!("收包线程已启动");
        while !shutdown_recv.load(Ordering::Relaxed) {
            match rx.next() {
                Ok(frame) => {
                    stats_recv.add_rx_bytes(frame.len() as u64);
                    if let Some(parsed) = parse_and_filter(frame, &vni_set, &stats_recv) {
                        if frame_tx.blocking_send(parsed).is_err() {
                            tracing::warn!("转发通道已关闭，收包线程退出");
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("接收数据错误: {}", e);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        tracing::info!("收包线程已退出");
    });

    // 异步转发循环
    while !shutdown.load(Ordering::Relaxed) {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("收到 Ctrl+C 信号，正在停止...");
                shutdown.store(true, Ordering::Relaxed);
                break;
            }
            frame = frame_rx.recv() => {
                match frame {
                    Some(f) => {
                        if let Err(e) = fwd.send(f).await {
                            tracing::warn!("发送失败: {}", e);
                        }
                    }
                    None => {
                        tracing::info!("转发通道已关闭");
                        break;
                    }
                }
            }
        }
    }
}
