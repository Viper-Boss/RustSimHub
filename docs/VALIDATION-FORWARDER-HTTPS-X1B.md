# X1B 原生转发HTTPS投递组件

日期：2026-10-24。源码及本机真实TCP/TLS验证完成；没有在棒子部署或启用转发服务。

## 实现

新增`crates/forwarder/src/delivery.rs`及回环测试，采用已有Hyper HTTP/1.1与rustls。每次调用只做一次POST，连接/权限核对/TLS/HTTP/响应受同一10秒异步截止控制，不自动重试、重连或跟随重定向；没有连接池、环境代理或隐式DNS/默认路由。所有传输future及TLS socket由一次调用持有，没有脱离调用存活的后台Hyper driver；取消或主管通道断开会丢弃该连接。

只接受有限根证书集合（最多512/合计1MiB），固定证书链和主机名校验，HTTP/1.1 ALPN；不提供危险验证器或跳过检查开关。限定JSON请求32768字节、响应8192字节/头部8192字节/32头；不解压响应。HTTP库会接受文本上越界的端口并让port_u16返回None，测试发现它会被误当443，已在配置与传输共用地址验证中明确拒绝。

Connector和Fence都是必须由调用者提供的接口：分别负责真实已批准出口及当前Core Owner/realm/配置代次/唯一attempt/SIM核对，连接前、TLS前、提交前各复核。回调必须使用非阻塞、有界异步实现。当前只有测试适配器；没有生产出口选择、Root主管或真实worker IPC，不能把本组件当成已实现的防蜂窝回退服务。客户端本身不提供绕过这些接口的默认联网路径。

应用提交前拒绝/取消/超时为NotSubmitted；进入唯一send_request后任何回执丢失、取消、超时或不确认都为Unknown，不据5xx、302或200中的错误JSON自动重投。Confirmed表示目标协议确认，不等于用户已阅读。与原持久队列结合时使用精确Claim完成回执；停用/改代次后的迟到回执仍受原队列约束。

## 验证

16项新增真实回环HTTPS测试，转发组件共40项，完整385项Rust测试、全工作区严格Clippy、生产Core无默认特征检查和ARM64库构建通过。

- 实际受信任TLS POST：固定路径、JSON、Bearer和delivery ID，唯一连接/唯一请求。
- 不信任证书、域名不匹配，以及三个阶段的权限拒绝都不发送应用请求。
- 初始取消/主管消失、TLS握手中取消、已经收到请求后取消，检查socket关闭且没有孤立driver。
- 阻塞fence与已提交等待均有总截止；302/503不重连、不跟随、不重发。
- 固定长度/分块响应过大、过多/过大响应头、压缩响应、提供方错误JSON拒绝，不把未知判作成功；静态错误码不拷贝服务端原文或凭据。
- 真实签名Auth Owner→真实Store来信→原子queue claim→实际HTTPS ACK→唯一终态回执。
- TLS已经建立后，在实际权限库撤销Owner，提交前拒绝；已收到请求但丢回执时队列保留Unknown，不再派发。

全部为本机loopback、内存临时证书及合成消息，不连接Bark/Telegram/用户真实Webhook，不发送或转发真实短信。回环服务的简单HTTP读取器仅在测试文件内；生产解析使用Hyper。最初缺缓存依赖、测试借用/命名错误及端口边界失败日志均保留，最后通过的源码不能冒充这些早期失败已成功。

命令（Rust1.98.0）：cargo test --workspace --all-features --locked --offline；cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings；cargo check -p rustsimhub-core --no-default-features --locked --offline；cargo build -p rustsimhub-forwarder --release --target aarch64-unknown-linux-gnu --locked --offline。

最终本地日志：`.private/forwarder-https-workspace-20261024-first.log`与exit=0。精确提交`ecd3545cbbc2afe0651d6922d89880bcbaed5ba3`的[CI40088053542](https://github.com/Viper-Boss/RustSimHub/actions/runs/40088053542)两个任务均success，包括385项Rust、13组网页/系统夹具、三个实际材料CLI及Native音频。ARM64库已保存并经readelf核对AArch64，SHA-256为`cc0cb84b5d4b030441290941ed7568712c40ab71d69abc5346ff4160341b3243`。它是库构建证据，不是设备安装或转发服务验收。

## 还缺

实际Core命令/worker IPC、明确且绑定出口的Connector、独立worker进程/Root启停主管、关闭后无PID/子进程/触发器及冷启动验收、网页开关/规则/回执、签名交付、真实目标递送；邮件及其余通道/正则/时间窗仍留在总清单。后续要将本组件接到这些真实控制流程，不能以单个fence布尔值或本地enabled标志代替实际权限和停机证明。

本轮短信网页的一次10010/CXYE实机验收属于既有epoch46产品，独立记录在P2H；未使用本组件向任何人转发。新增范围单列，原P0–P7因P2H达到80%，本组件源码本身不额外加分。
