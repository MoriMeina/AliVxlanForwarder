# AliVxlan Forwarder

> 高性能 Alibaba VPC VXLAN 协议解析与报文还原工具

AliVxlan Forwarder 是一款专为阿里云私网隧道（AliVxlan）流量解析设计的轻量级工具，具备稳定的协议解析能力与强大的报文还原输出能力。适用于云上流量旁路分析、安全监测、虚拟化可视化等多种场景。

---

## ✨ 核心功能

### 📦 协议解析能力

- **支持 Alibaba VPC Tunnel 协议结构**
- 自动识别 VXLAN 报文头及字段
- 可提取 VNI（Tunnel ID）、Payload Type 等关键字段
- 解包支持两类负载：
  - **IP Over IP**（三层转发）
  - **MAC Over IP**（二层转发）
- **支持 802.1Q VLAN 标签**（含 QinQ 嵌套），自动跳过 VLAN 头解析内层报文

---

### 🎯 精准 VNI 过滤

- 支持配置 **单个或多个 VNI**
- 默认模式下抓取 **全部 VNI 流量**
- 可应用于 VPC 中转分析、多租户隔离解析等场景

---

### 🚀 报文重构与输出

- **IP 报文**：还原 IP 报文并封装成完整以太网帧
- **MAC 报文**：原样输出完整二层以太网帧
- 三种输出模式：
  - `--tap`：写入 TAP 设备，实现完整链路模拟
  - `--output`：发送到物理网卡，直接转发
  - `--pcap`：写入 PCAP 文件，离线分析

---

### 📊 监控与可观测性

- **REST API**：`/stats` 端点返回 JSON 格式的收发字节、速率、丢包计数
- **Prometheus metrics**：`/metrics` 端点输出标准 Prometheus 格式，方便集成 Grafana / Prometheus 监控
- **分类丢包统计**：区分 VNI 过滤、解析错误、发送失败、报文过短四类丢包
- **结构化日志**：基于 `tracing`，支持 `RUST_LOG` 环境变量和 `--log-level` 参数控制级别

---

### ⚙️ 部署优势

- 🧵 **异步架构**：阻塞收包隔离到独立线程，转发通过 mpsc channel 异步处理
- 🔒 零侵入设计，适配镜像接口旁路部署
- 🧰 自动接口启动，简化配置过程
- 🛠 支持 CLI 传参，快速集成运维流程
- 📋 支持 systemd 服务管理，开机自启、崩溃自动重启

---

## 🧪 示例用法

### TAP 模式（链路模拟）

```bash
./AliVxlanForwarder \
    --input eth0 \
    --tap tap0 \
    --vni 287683 --vni 100000
```

### 网卡转发模式

```bash
./AliVxlanForwarder \
    --input eth0 \
    --output eth1
```

### PCAP 捕获模式

```bash
./AliVxlanForwarder \
    --input eth0 \
    --pcap /tmp/capture.pcap
```

### 完整参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--input` | 抓包网卡名（必填） | — |
| `--tap` | TAP 接口名（与 `--output`、`--pcap` 三选一） | — |
| `--output` | 转发到物理网卡（与 `--tap`、`--pcap` 三选一） | — |
| `--pcap` | 写入 PCAP 文件（与 `--tap`、`--output` 三选一） | — |
| `--vni` | 允许的 VNI 列表，可重复传入 | 全部通过 |
| `--api-addr` | REST API 监听地址 | `127.0.0.1:8080` |
| `--log-level` | 日志级别 (error/warn/info/debug/trace) | `info` |

---

## 📡 REST API

### `GET /stats`

```json
{
  "rx_bytes": 2906538955,
  "tx_bytes": 499045058,
  "drop_count": 0,
  "drops_vni_filtered": 0,
  "drops_parse_error": 0,
  "drops_send_failed": 0,
  "drops_too_short": 0,
  "rx_bps": 2151942884.38,
  "tx_bps": 363955159.47
}
```

### `GET /metrics`

Prometheus 格式，包含 `alivxlan_rx_bytes_total`、`alivxlan_tx_bytes_total`、`alivxlan_rx_bps`、`alivxlan_tx_bps`、分类丢包计数器等指标。

---

## 🔧 systemd 部署

```bash
# 复制二进制
sudo cp AliVxlanForwarder /usr/bin/

# 创建 service 文件
sudo cp alivxlan.service /etc/systemd/system/

# 启用并启动
sudo systemctl daemon-reload
sudo systemctl enable --now alivxlan

# 查看状态
sudo systemctl status alivxlan

# 查看日志
sudo journalctl -u alivxlan -f
```

---

## ⚠️ 注意事项

- `--tap`、`--output`、`--pcap` 三个参数互斥，只能指定其中一个
- `--api-addr 0.0.0.0:8080` 监听所有网卡，`127.0.0.1:8080` 仅监听本机
- 程序依赖 `ip` 命令（iproute2）来管理接口状态
- 退出时自动清理 TAP 接口（Ctrl+C 优雅退出）
