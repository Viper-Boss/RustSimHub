# NAS 兼容契约基础

`crates/compat` 现已实现签名契约、离线历史迁移、授权历史查询与可选 NAS 只读会话桥。启用方式和准确接口见 [nas-online-bridge.md](nas-online-bridge.md)，迁移与查询见 [history-query.md](history-query.md)。现有 NAS 需要专用适配器；原页面和 PCM 尚未对接。
签名验证必须同时满足 owner 批准的当前 NAS principal、scope/generation、设备绑定证明、时间窗口和持久重放栅栏，才能获得兼容读取会话。

保留 AirSimHub protocol 1.5 的 method/path/timestamp/nonce/body SHA256 的签名字节、
NSH-410 公钥派生标识、公钥指纹以及版本兼容判断方向。字段有长度、字符与 Base64
规范限制，拒绝换行注入、弱公钥和不规范签名。原协议签名不绑定设备 ID，
只读网关额外固定设备公钥、绑定 epoch 与专用 NAS principal，不能把旧签名直接作为 v2 token 或 owner 初始化凭据。
会话使用本地 UTC ±300 秒时间窗口、持久 nonce 和时钟高水位；重启清空兼容 token，保留重放栅栏。接口默认关闭，只能显式启用。

有限读取通过 `POST /compat/v1/read` 提供状态、短信、短信详情和通话记录，且要求当前 principal 的 kind=Nas、
active=true、所需 scope 已由 owner 批准；调用者仍须经 Core 检查当前代次和撤销。
旧配对必须走 owner 审批；短信发送、电话操作、OTA、恢复出厂、PQ/KCP 均返回未支持。
没有从旧 NAS 迁移所有者、私钥或运行中的 SIM 线路状态。

对照向量通过精确旧提交 ea019485e5d890a96acae357c52b94c746de837f 的原始 proto 模块生成。
`tools/compat-golden/main.go` 使用纯合成种子和内容；NAS 上现有 Go 1.26.5 镜像在
network=none、只读 rootfs、UID 1000、限定 CPU/内存的容器内执行，无外部依赖下载。
包含 5 组正文（空、JSON、中文、二进制）签名/标识向量与 12 组版本判断。
Rust 的反向检查放在 legacy_golden.rs。f536c63 精确源码在 NAS ARM64 无网络容器的
5 项测试全部通过，CI 全工作区测试及生产 feature 检查通过。首次 CI 的两项格式化
Clippy 建议（match guard 与 as_chunks）已修复，待新提交 CI 复核；NAS Rust 镜像没有
Clippy，不为此安装工具，静态检查由固定工具链 CI 完成。这是早期契约验证记录；当前实际接入结果另见 VALIDATION-NAS-BRIDGE-P5D.md。

NAS 原应用页面、写操作与 PCM 仍需实现和验收。版本不兼容返回具体原因，不能伪装成设备离线；data_format=2 不冒充完整旧 Agent 数据格式。
