use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ali-vxlan-tap-forwarder")]
pub struct Args {
    #[arg(long, help = "抓包网卡名，如 eth0")]
    pub input: String,

    #[arg(long, help = "使用 TAP 接口进行转发，如 tap0")]
    pub tap: Option<String>,

    #[arg(long, help = "使用物理接口进行转发，如 eth0")]
    pub output: Option<String>,

    #[arg(long, help = "指定允许的 VNI（可重复传入）", num_args=0..)]
    pub vni: Vec<u32>,
}

impl Args {
    pub fn validate(&self) {
        match (&self.tap, &self.output) {
            (Some(_), Some(_)) => {
                panic!("--tap 和 --output 只能传入一个");
            }
            (None, None) => {
                panic!("必须传入 --tap 或 --output 其中一个");
            }
            _ => {}
        }
    }
}
