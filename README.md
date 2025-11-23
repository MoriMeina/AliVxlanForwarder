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

---

### 🎯 精准 VNI 过滤

- 支持配置 **单个或多个 VNI**
- 默认模式下抓取 **全部 VNI 流量**
- 可应用于 VPC 中转分析、多租户隔离解析等场景

---

### 🚀 报文重构能力

- **IP 报文**
  - 还原 IP 报文并封装成完整以太网帧
- **MAC 报文**
  - 原样输出完整二层以太网帧
- 支持写入 TAP 设备，实现 **完整链路模拟**

---

### ⚙️ 部署优势

- 🧵 **Tokio 异步架构**，高并发处理大流量
- 🔒 零侵入设计，适配镜像接口旁路部署
- 🧰 自动接口启动，简化配置过程
- 🛠 支持 CLI 传参，快速集成运维流程

---

## 🧪 示例用法

```bash
./AliVxlanForworder \
    --input eth0 \
    --tap tap0 \
    --vni 287683 --vni 100000
