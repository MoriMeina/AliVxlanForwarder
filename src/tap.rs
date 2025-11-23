use std::{
    fs::File,
    io,
    mem,
    os::fd::{AsRawFd, RawFd},
    process::Command,
};

use libc::{c_char, c_short, ifreq, IFF_NO_PI, IFF_TAP, TUNSETIFF};

pub struct TapInterface {
    name: String,
    file: File,
}

impl TapInterface {
    pub fn create(name: &str) -> io::Result<Self> {
        println!("[*] 打开 /dev/net/tun...");
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/net/tun")
        {
            Ok(f) => {
                println!("[+] 成功打开 /dev/net/tun");
                f
            }
            Err(e) => {
                eprintln!("[!] 打开 /dev/net/tun 失败: {}", e);
                return Err(e);
            }
        };

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

        println!("[*] 执行 ioctl 创建 TAP 接口 {}...", name);
        let res = unsafe { libc::ioctl(fd, TUNSETIFF, &ifr as *const _ as *const libc::c_void) };
        if res < 0 {
            let err = io::Error::last_os_error();
            eprintln!("[!] ioctl 创建 TAP 接口失败: {}", err);
            return Err(err);
        }
        println!("[+] ioctl 创建 TAP 接口成功");

        // 设置接口为 up
        println!("[*] 设置接口 {} 为 UP...", name);
        let status = Command::new("ip")
            .args(["link", "set", name, "up"])
            .status()?;
        if !status.success() {
            eprintln!("[!] 设置接口 {} 为 UP 状态失败", name);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("设置接口 {} 为 UP 状态失败", name),
            ));
        }
        println!("[+] 接口 {} 设置为 UP", name);

        println!("[+] TAP 接口 {} 创建成功", name);
        Ok(Self {
            name: name.to_string(),
            file,
        })
    }

    pub fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

impl Drop for TapInterface {
    fn drop(&mut self) {
        println!("[*] 正在关闭 TAP 接口 {}", self.name);
        let _ = Command::new("ip")
            .args(["link", "set", &self.name, "down"])
            .status();

        let _ = Command::new("ip")
            .args(["link", "delete", &self.name])
            .status();
        println!("[*] TAP 接口 {} 已删除", self.name);
    }
}
