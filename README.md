# Ping Tunnel

一个基于 QUIC 协议的高性能网络隧道工具，支持将 TCP 流量通过 QUIC 协议进行转发。

## 项目简介

Ping Tunnel 是一个使用 Rust 开发的网络隧道工具，利用 QUIC 协议的优势（低延迟、多路复用、内置加密）来实现高效的流量转发。项目包含两个核心组件：

- **Edge (客户端)**: 连接到服务器，将本地流量通过 QUIC 隧道转发
- **Supernode (服务器)**: 接收 QUIC 连接，并将流量转发到目标 TCP 服务

## 特性

- 🚀 **高性能**: 基于 QUIC 协议，提供低延迟和高吞吐量
- 🔒 **安全**: 内置 TLS 加密，支持证书认证
- 🔑 **认证**: 支持 Token 认证机制
- 🌐 **灵活转发**: 支持动态指定转发目标
- 📦 **Node.js 支持**: 提供 Node.js 原生绑定，可在 Node.js 项目中使用
- ⚡ **异步架构**: 基于 Tokio 异步运行时，支持高并发

## 架构

```
┌─────────┐         QUIC          ┌──────────┐         TCP          ┌─────────┐
│  Edge   │ <───────────────────> │ Supernode│ <──────────────────> │  NGINX  │
│(Client) │                       │ (Server) │                      │ Service │
└─────────┘                       └──────────┘                      └─────────┘
```

## 快速开始

### 前置要求

- Rust 1.70+ (推荐使用 [rustup](https://rustup.rs/) 安装)
- Node.js 10+ (如果使用 Node.js SDK)

### 编译

```bash
# 克隆项目
git clone <repository-url>
cd ping-tunnel

# 编译项目
cargo build --release
```

### 生成证书

服务器需要 TLS 证书。你可以使用以下命令生成自签名证书：

```bash
# 创建证书目录
mkdir -p cert

# 生成私钥
openssl genrsa -out cert/key.pem 2048

# 生成证书
openssl req -new -x509 -key cert/key.pem -out cert/cert.pem -days 365
```

### 运行服务器 (Supernode)

```bash
cargo run --bin supernode -- \
  "0.0.0.0:4433" \
  "0.0.0.0:4432" \
  "./cert/cert.pem" \
  "./cert/key.pem"
```

参数说明：

- `quic_bind_addr:port`: QUIC 服务器监听地址
- `tcp_bind_addr:port`: TCP 服务器监听地址（接收 HTTP 请求）
- `cert_path`: 证书文件路径
- `key_path`: 私钥文件路径

### 运行客户端 (Edge)

```bash
cargo run --bin edge -- \
  "127.0.0.1:4433" \
  "my-secret-token" \
  "127.0.0.1:8080"
```

参数说明：

- `server_addr`: 服务器地址
- `token`: 认证 Token
- `forward_to`: 转发目标地址

## 预编译二进制

你也可以直接从 GitHub Releases 下载已经构建好的二进制和 Node.js 插件，文件命名约定如下：

| 平台              | 文件名                                                            |
| ----------------- | ----------------------------------------------------------------- |
| Linux x86_64 gnu  | `edge-linux-x64.node`                                             |
| Linux x86_64 musl | `edge-linux-x64`, `supernode-linux-x64`                           |
| macOS x86_64      | `edge-darwin-x64.node`, `edge-darwin-x64`, `supernode-darwin-x64` |
| macOS arm64       | `edge-darwin-arm.node`, `edge-darwin-arm`, `supernode-darwin-arm` |
| Windows x86_64    | `edge-win-x64.node`, `edge-win-x64.exe`, `supernode-win-x64.exe`  |

## Node.js SDK

### 本地构建

```bash
# 在项目根目录
npm install
npm run build  # 为当前平台构建 Node.js 原生插件，输出到 nodejs/edge.node
```

### 使用示例（直接使用当前仓库）

```javascript
const { EdgeClient } = require('./nodejs');

// 创建客户端
const client = new EdgeClient(
  '127.0.0.1:4433', // 服务器地址
  'my-secret-token', // 认证 Token
  '127.0.0.1:8080' // 转发目标
);

// 连接到服务器
client.connect();

// 周期性获取本地 inbound 地址
setInterval(async () => {
  const addr = await client.getInboundAddr();
  console.log('inbound addr:', addr);
}, 1000);
```

如果之后发布为 npm 包，可以在其他项目中这样使用：

```javascript
const { EdgeClient } = require('ping_tunnel_edge');
```

## 工作原理

### 连接流程

1. **客户端连接**: Edge 客户端通过 QUIC 连接到 Supernode 服务器
2. **认证**: 客户端发送认证 Token 进行身份验证
3. **心跳保持**: 客户端定期发送 Ping 消息保持连接
4. **流量转发**:
   - 服务器接收 HTTP 请求（通过 TCP）
   - 根据请求头中的 Token 找到对应的 QUIC 连接
   - 将请求转发到客户端指定的目标地址

### 协议格式

项目使用自定义的隧道协议，支持以下命令：

- `Ping/Pong`: 心跳检测
- `Auth/AuthResult`: 身份认证
- `Forward`: 流量转发
- `SetSessionMeta`: 设置会话元数据

### HTTP 请求头

客户端可以通过 HTTP 请求头控制转发行为：

- `X-Tunnel-Token`: 认证 Token（必需）
- `X-Tunnel-Forward-To`: 动态转发目标（可选，覆盖默认转发地址）

## 开发

### 项目结构

```
ping-tunnel/
├── src/
│   ├── lib.rs          # 库入口，包含 Node.js 绑定和公共导出
│   ├── edge.rs         # Edge 客户端二进制入口
│   ├── supernode.rs    # Supernode 服务器二进制入口
│   ├── transport/      # 传输抽象与 QUIC 实现
│   └── tunnel/         # Edge/Supernode/会话/转发 等隧道逻辑
├── nodejs/             # Node.js 绑定包装与示例
│   ├── index.js
│   ├── index.d.ts
│   └── example.js
└── cert/               # 证书目录
```

### 构建 Node.js 绑定

```bash
# 构建发布版本
npm run build

# 构建调试版本
npm run build:debug
```

## 依赖

主要依赖：

- **quinn**: QUIC 协议实现
- **tokio**: 异步运行时
- **rustls**: TLS 实现
- **napi**: Node.js 原生 API 绑定
- **dashmap**: 并发哈希映射

## 注意事项

- 生产环境请使用有效的 TLS 证书，不要使用自签名证书
- Token 应该足够复杂，建议使用随机生成的字符串
- 服务器会定期清理不活跃的连接（默认 60 秒无心跳）
