use async_trait::async_trait;
use pnet::datalink::{self, Channel::Ethernet};
use pnet::packet::{
    ethernet::EthernetPacket, ip::IpNextHeaderProtocols, ipv4::Ipv4Packet, udp::UdpPacket, Packet,
};
use std::{collections::HashSet, process::Command, sync::Arc};
use std::sync::atomic::Ordering;

use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use std::os::fd::FromRawFd;

use crate::stats::Stats;

/// 确保网络接口为 UP 状态
fn ensure_interface_up(name: &str) {
    let iface = datalink::interfaces()
        .into_iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("找不到接口 {}", name));

    if !iface.is_up() {
        eprintln!("[*] 接口 {} 是 DOWN，尝试设置为 UP...", name);
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

pub struct TapForwarder {
    file: Mutex<tokio::fs::File>,
    stats: Arc<Stats>,
}

impl TapForwarder {
    pub fn new(fd: i32, stats: Arc<Stats>) -> Self {
        let std_file = unsafe { std::fs::File::from_raw_fd(fd) };
        let file = tokio::fs::File::from_std(std_file);
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
            .expect("找不到接口");

        let (tx, _) = match datalink::channel(&iface, Default::default())
            .expect("创建 channel 失败")
        {
            Ethernet(tx, _rx) => (tx, ()),
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
                self.stats.drop_count.fetch_add(1, Ordering::Relaxed);
                Err(std::io::Error::new(std::io::ErrorKind::Other, format!("发送失败: {:?}", e)))
            }
            None => {
                self.stats.drop_count.fetch_add(1, Ordering::Relaxed);
                Err(std::io::Error::new(std::io::ErrorKind::Other, "发送失败: None 返回"))
            }
        }

    }
}

/// 异步主转发循环，完全异步
pub async fn run_forwarder(args: &crate::args::Args, fwd: Arc<dyn Forwarder>, stats: Arc<Stats>) {
    ensure_interface_up(&args.input);

    let iface = datalink::interfaces()
        .into_iter()
        .find(|i| i.name == args.input)
        .expect("找不到输入接口");

    let (_, mut rx) = match datalink::channel(&iface, Default::default()).expect("创建channel失败") {
        Ethernet(_tx, rx) => (_tx, rx),
        _ => panic!("只支持Ethernet通道"),
    };

    let vni_set = if args.vni.is_empty() {
        None
    } else {
        Some(args.vni.iter().copied().collect::<HashSet<_>>())
    };

    loop {
        match rx.next() {
            Ok(frame) => {
                stats.add_rx_bytes(frame.len() as u64);

                if let Some(eth) = EthernetPacket::new(frame) {
                    if let Some(ipv4) = Ipv4Packet::new(eth.payload()) {
                        if ipv4.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
                            if let Some(udp) = UdpPacket::new(ipv4.payload()) {
                                if udp.get_destination() == 250 {
                                    let p = udp.payload();

                                    if p.len() >= 8 {
                                        let vni = ((p[4] as u32) << 16) | ((p[5] as u32) << 8) | (p[6] as u32);

                                        if let Some(ref vs) = vni_set {
                                            if !vs.contains(&vni) {
                                                stats.drop_count.fetch_add(1, Ordering::Relaxed);
                                                continue;
                                            }
                                        }

                                        let inner = &p[8..];
                                        let payload_type = (p[1] & 0x08) >> 3;

                                        let frame_to_send = match payload_type {
                                            0 if inner.len() >= 20 => {
                                                let mut buf = Vec::new();
                                                buf.extend_from_slice(&[0x02, 0, 0, 0, 0, 1]);
                                                buf.extend_from_slice(&[0x02, 0, 0, 0, 0, 2]);
                                                buf.extend_from_slice(&[0x08, 0x00]);
                                                buf.extend_from_slice(inner);
                                                buf
                                            }
                                            1 if inner.len() >= 14 => inner.to_vec(),
                                            _ => {
                                                stats.drop_count.fetch_add(1, Ordering::Relaxed);
                                                continue;
                                            }
                                        };

                                        if let Err(e) = fwd.send(frame_to_send).await {
                                            eprintln!("发送失败: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("接收数据错误: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }
}
