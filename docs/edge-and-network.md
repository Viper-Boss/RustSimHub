# HTTPS 调试入口与网络只读状态

Rust `rustsimhub-edge` 提供设备本地 HTTPS、静态调试网页和 `/api/v2` POST。业务仍由 core 重新鉴权。
没有手机原生 app、WebSocket / WebRTC 或远程 relay；网页目前通过显式刷新与分页补同步事件。
实现参考 [axum](https://docs.rs/axum/0.8.9/axum/) 与 [tokio-rustls](https://docs.rs/tokio-rustls/0.26.6/tokio_rustls/)。

## 部署与信任边界

```text
rustsimhub-edge --init-tls NEW_PRIVATE_DIRECTORY DNS_NAME_OR_IP
rustsimhub-edge --serve PRIVATE_CONFIG_JSON
rustsimhub-core --build-info
simhubctl --build-info
rustsimhub-edge --build-info
simhubctl network CONTROL_SOCKET PRIVATE_SESSION_FILE
```

config 须由 edge UID 所有，父目录 0700，普通文件 0600 或更严格，不能为 symlink / hardlink。
字段为 listen、origin、core_socket、core_uid、tls_directory；未知字段拒绝。
listen 是明确的本地地址与非零端口，拒绝通配及公网地址；origin 必须是完整、规范 HTTPS origin，不带路径、query 或用户信息。
实际地址和凭据只写私有配置，不放进 Git。

TLS 生成新目录和证书/私钥，不覆盖旧文件。此证书是自签名开发证书；必须通过可信渠道核对 SHA-256 或部署受信任证书。
浏览器设备公钥的签名检查不能取代网页本身的 TLS 信任，否则攻击者可以替换网页代码。
真正生产设备的证书导入/更新、所有者首启向导与安装仍待完成。

core 与 edge 使用不同的锁定系统 UID，专用 IPC group 仅能遍历私有 socket 父目录；目录不可 group/other 写。
edge 每次检查 core socket 类型、父目录/UID 和 SO_PEERCRED；固定 core_uid 改变就拒绝。
edge 不打开业务库、device identity、ALSA 或 system D-Bus。实机单元使用只读配置挂载、PrivateDevices、PrivateTmp 和隐藏 D-Bus 路径。
入口账户没有 modem/音频组权限；不能以执行远端请求为理由增添 shell、AT 或任意文件接口。

## HTTP 与有界资源

请求格式仍为 wire v2 `{protocol,id,command}`，服务器复用严格有限 AuthCommand 解析，拒绝角色注入及未知字段。
Host 必须精确匹配配置 authority；API 还要求精确唯一 Origin、application/json、X-RustSimHub-Request: 2，且不带 query。
没有 Cookie 会话或宽松 CORS。`IssueMedia` 在网络入口拒绝，直到媒体适配器能绑定实际 DTLS 对端。
所有 TLS/IPC 错误输出有限标签；不日志化请求、token、手机号、短信或 SDP。

TLS 握手 3 秒；24 个连接、32 个请求头、16KiB HTTP header 缓冲；每连接最多 90 秒。
API 请求体/核心响应最多 65536 字节；最多 8 个并发转发；整体 120 次 API 请求/分钟；核心调用 15 秒。
关闭时停止接收并取消本进程连接；已经提交的核心持久操作不重试，客户端查询 Unknown。
这些是资源边界，不能称已通过长时间公网 DoS 或全部依赖漏洞审查。

响应带 no-store、nosniff、no-referrer、DENY frame、严格 CSP；网页没有第三方脚本和动态 innerHTML。
当前 microphone/camera 权限禁用，因为还没有浏览器语音功能。

## 浏览器客户端

WebCrypto Ed25519 私钥设为不可导出并保存在本机 IndexedDB；公钥供所有者核对批准。
设备 pin 由用户通过可信 USB/CLI 核对，不自动信任网络返回的公钥。登录 token 只留内存。
客户端先验证 device signature，再检查精确 transcript 字节对应的 domain、purpose、双方 key、challenge/instance、kind、scope、generation。
只签 pair/login，不提供任意消息签名。Node WebCrypto 用同一个 proof.js 跑真实算法回归；实际浏览器交互还待验收。

网页有登录/配对申请、状态、短信摘要和正文、事件分页、短信准备/提交、呼叫准备/提交、操作查询和租约续期。
提交前把 marker 事务写入 IndexedDB；断线、刷新或超时不会自动再次提交。结果不明保留操作 ID，只能查询。
准备和结果查询本身仍受 core scope、SIM epoch、独占租约及单次提交门禁保护。
网页写功能代码存在；当前硬件 HTTPS 验收只使用只读 core，没有新短信或拨号，不能把静态页面交付称为网页写功能验收。

## NetworkStatus

`{"op":"network_status","token":"..."}` 需要 status.read；硬件查询前后均再次核验当前 Principal，撤销/代次改变拒绝返回。
同时最多两个读取，总期限 5 秒；固定唯一 NetworkManager owner，在查询前后检查 owner 未换。
GetDevices 最多 16 个，拒绝重复或外来路径；只读取有限 state、managed、设备类型、本地 IPv4 与全局网络/radio/connectivity/metered 属性。
不读取密码、SSID、BSSID、MAC、连接名称、APN 或任意 settings，不扫描、不激活、不写网络配置。
仅输出本地 IPv4；公网地址不返回。普通状态变化可能发生于读取过程中，这不是全属性事务快照。
`cellular_enabled` 当前对应 NM 的 WWAN radio 开关，不能当成蜂窝数据授权或已联网；不允许靠关闭 WWAN 来切断电话/短信。

实机使用 NM 1.52。API 参考 [NetworkManager](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html)、
[Device](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html)、[IP4Config](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.IP4Config.html)，
以目标板真实属性和测试为准，不假定上游最新版功能已安装。

## 构建归属

源包使用允许列表；新增 edge/src/web 下的 html/js/css。`--build-info` 输出编译时源包 SHA-256，未提供构建环境时为 null。
NAS 解压使用当前时间，避免文件在上一轮长编译期间修改、随后解压仍旧于旧产物时间而被 Cargo 误复用。
构建器将源包 SHA 注入被 Cargo 跟踪的 compile-time env；部署前每个程序自报必须与源包相同，同时校验传输的二进制哈希。
这证明选定源包进入该构建，不是可复现构建、签名更新或完整供应链审核的替代品。

验证见 VALIDATION-EDGE-P1.md。后续推进实际浏览器、网页业务、Wi-Fi / 蜂窝策略及安全部署。

用户最新顺序约束：当前 SIM 无流量，蜂窝数据实测放最后。网络写流程先实现/验收 USB 与 Wi-Fi；预算/漫游/蜂窝授权使用模拟后端，不激活普通 APN，也不进行蜂窝互联网探测。WWAN radio enabled 不是流量已获授权，不能作为激活条件。
