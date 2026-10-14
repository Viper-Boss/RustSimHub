# NAS 兼容契约基础

`crates/compat` 是 Rust 契约库，尚未启用旧协议监听入口、会话或迁移工具。
不能将签名验证通过等同于已获得 RustSimHub 业务权限，也不能据此宣称现有 NAS 可直接接入。

保留 AirSimHub protocol 1.5 的 method/path/timestamp/nonce/body SHA256 的签名字节、
NSH-410 公钥派生标识、公钥指纹以及版本兼容判断方向。字段有长度、字符与 Base64
规范限制，拒绝换行注入、弱公钥和不规范签名。原协议签名不绑定设备 ID，
未来网关必须额外固定设备公钥与专用 NAS principal，不能把它直接作为 v2 token 或 owner 初始化凭据。
当前没有时间窗口/持久重放库/会话授权实现，不开放网络接口。

有限接口映射只允许 GET 状态、短信和通话记录，且要求当前 principal 的 kind=Nas、
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
Clippy，不为此安装工具，静态检查由固定工具链 CI 完成。当前不计 P5 实机兼容点数。

接下来是独立只读适配器、时间/重放账本、owner 批准与撤销贯穿验证，随后才是旧 NAS
实际连接与去重导入。版本不兼容必须返回具体原因，不能伪装成设备离线。
