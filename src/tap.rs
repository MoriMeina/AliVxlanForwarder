use std::{
    fs::File,
    io,
    mem,
    os::fd::AsRawFd,
    process::{Command, Stdio},
};

use libc::{c_char, c_short, ifreq, IFF_NO_PI, IFF_TAP, TUNSETIFF};

pub struct TapInterface {
    name: String,
    file: Option<File>,
}

impl TapInterface {
    pub fn create(name: &str) -> io::Result<Self> {
        tracing::info!("打开 /dev/net/tun ...");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/net/tun")?;

        let fd = file.as_raw_fd();
        let mut ifr: ifreq = unsafe { mem::zeroed() };

        let name_bytes = name.as_bytes();
        if name_bytes.len() >= libc::IFNAMSIZ {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TAP 接口名过长",
            ));
        }

        for (i, &b) in name_bytes.iter().enumerate() {
            ifr.ifr_name[i] = b as c_char;
        }

        ifr.ifr_ifru.ifru_flags = (IFF_TAP | IFF_NO_PI) as c_short;

        tracing::info!("执行 ioctl 创建 TAP 接口 {}...", name);
        let res = unsafe { libc::ioctl(fd, TUNSETIFF, &ifr as *const _ as *const libc::c_void) };
        if res < 0 {
            let err = io::Error::last_os_error();
            tracing::error!("ioctl 创建 TAP 接口失败: {}", err);
            return Err(err);
        }

        tracing::info!("设置接口 {} 为 UP...", name);
        let status = Command::new("ip")
            .args(["link", "set", name, "up"])
            .status()?;
        if !status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("设置接口 {} 为 UP 状态失败", name),
            ));
        }

        tracing::info!("TAP 接口 {} 创建成功", name);
        Ok(Self {
            name: name.to_string(),
            file: Some(file),
        })
    }

    /// 取出底层 File，转移 fd 所有权。调用后 Drop 不再关闭 fd。
    pub fn take_file(&mut self) -> File {
        self.file.take().expect("TAP file already taken")
    }
}

impl Drop for TapInterface {
    fn drop(&mut self) {
        tracing::info!("正在清理 TAP 接口 {}", self.name);
        let _ = Command::new("ip")
            .args(["link", "set", &self.name, "down"])
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("ip")
            .args(["link", "delete", &self.name])
            .stderr(Stdio::null())
            .status();
        tracing::info!("TAP 接口 {} 已删除", self.name);
    }
}
