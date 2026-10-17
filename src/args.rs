use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ali-vxlan-tap-forwarder", about = "AliVxlan 协议解析与报文转发工具")]
pub struct Args {
    #[arg(long, help = "抓包网卡名，如 eth0")]
    pub input: String,

    #[arg(long, help = "使用 TAP 接口进行转发，如 tap0")]
    pub tap: Option<String>,

    #[arg(long, help = "使用物理接口进行转发，如 eth0")]
    pub output: Option<String>,

    #[arg(long, help = "输出到 PCAP 文件")]
    pub pcap: Option<String>,

    #[arg(long, help = "指定允许的 VNI（可重复传入）", num_args = 0..)]
    pub vni: Vec<u32>,

    #[arg(long, help = "REST API 监听地址", default_value = "127.0.0.1:8080")]
    pub api_addr: String,

    #[arg(long, help = "日志级别 (error/warn/info/debug/trace)", default_value = "info")]
    pub log_level: String,
}

impl Args {
    pub fn validate(&self) -> Result<(), String> {
        let output_count = [&self.tap, &self.output, &self.pcap]
            .iter()
            .filter(|o| o.is_some())
            .count();

        if output_count > 1 {
            return Err("--tap、--output、--pcap 只能指定其中一个".into());
        }
        if output_count == 0 {
            return Err("必须指定 --tap、--output 或 --pcap 其中一个".into());
        }

        if self.api_addr.parse::<std::net::SocketAddr>().is_err() {
            return Err(format!("--api-addr 格式无效: {}", self.api_addr));
        }

        Ok(())
    }
}
