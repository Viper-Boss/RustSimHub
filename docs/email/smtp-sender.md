# A-29：实际独立SMTP/TLS发送器候选

`crates/forwarder/src/email/`实现真实Rust SMTP/TLS客户端，不是返回固定成功的stub。当前是独立、显式调用的库：没有修改现有Destination/PolicyEdit/PreparedRequest/Claim/worker协议4/outbox3，没有生产Core授权适配或邮件开关/队列派发。兼容提案PR120仍待审核；本包不把新增本地类型当作已冻结的HTTP/IPC格式。实际安装、版本迁移及获准邮箱测试另行验收。

## 实际接口与隐私

`EmailConfig::new`接受私有host、username、password、from、to（均Secret），固定port465/implicit TLS，不接URL/任意端口/代理/密钥路径。host为<=253B ASCII DNS标签，拒绝IP字面量/空标签/尾点；一个ASCII dot-atom发件人和收件人，各<=254B/local-part<=64B，拒绝显示名、多个地址、CR/LF、控制字符及国际地址。用户名1..64B可见ASCII，密码1..256B可打印ASCII，保证AUTH命令<=512B；不支持SASLprep/Unicode账户、其他认证机制或SMTPUTF8。

`EmailRequest::prepare(config,delivery_id,message,share)`验证真实已有Message边界，拒绝binary/重复候选、超量/非法UUID/不合法时间。复用既有Share::Notice/MaskedText/FullText及真实masked_text；默认策略由宿主Owner决定，本方法不创造同意。Notice不含SMS发送者/正文，MaskedText不添加发送者，FullText仅在真实显式授权后使用。调用方仍负责以真实Core Fence核对本策略，不靠传入Share当鉴权。

Subject固定ASCII、Message-ID只绑定delivery UUID与固定`.invalid`域，SMS发送者/正文不进入头部。Date只从已提交Message的RFC3339时间经time标准库严格解析并格式化RFC2822，不能注入原字符串。UTF-8正文用标准base64编码、76字符CRLF换行，完整DATA<=16384B且各行<=512B；正文不能引入SMTP终止符、头或命令。没有附件/任意头/多个收件人/隐式降级/截断。

`EmailClaim::new`绑定完整Binding、delivery/attempt UUID、配置generation及Message的line，id必须匹配准备结果；元数据不是Owner证明。`fingerprint`对含全部私有目标/凭据/MIME/上下文的有限借用结构进行SHA256，临时JSON<=65536B且零化；这是候选本地不可变请求摘要，不是签名/授权或新生产数据版本。公开Outcome不含地址、账户、原始服务端文本或正文。所有Secret/MIME/SASL/base64/命令及逐行回复临时缓冲使用已有zeroize；不声称能擦除TLS/运行时内部所有副本。

## 一次尝试与阶段授权

```text
BeforeConnect → 固定Connector(host,465)
BeforeTls → 可信根+指定主机名验证
220 → EHLO/250（明确AUTH PLAIN）
BeforeAuthenticate → AUTH PLAIN/235
BeforeMailFrom → MAIL FROM/250
BeforeRecipient → RCPT TO/250
BeforeData → DATA/354
BeforeSubmit → MIME与终止符/最终250
```

每一步通过独立EmailFence核验**当前**Owner/realm/SIM/generation/attempt及实际fingerprint。没有默认允许实现；库未提供生产Fence。当前Core只允许原三步各一次，不能将这四个新增中间阶段都伪映射为重复BeforeSubmit；主线须先审核协议5的有限适配，再接真实生产。旧HTTPS发送器、正常短信授权和Root材料保持。

复用现有Connector；没有ambient DNS/socket/代理或普通蜂窝回退。证书只从显式可信根集合构建，使用rustls默认安全协议与严格主机验证，不接受自定义危险verifier、明文/STARTTLS降级或HELO回退。只尝试一次认证、一个收件人和一次DATA；无PIPELINING、BDAT、VRFY/EXPN、自动重连或重试。已缓冲的未发命令对应回复拒绝，不能将提前250作为以后阶段的确认。

每回复最多32行，每行含CRLF<=512B，整次尝试所有回复合计<=8192B；同一多行回复的代码必须一致，类型/分隔/裸LF/非ASCII文本/截断/错阶段/超量均有限错误。整个操作共享<=10秒单调预算，覆盖Fence、连接、TLS、所有读写；可缩短不能延长。除外层timeout外，在每次外部回调返回/写入前再次核对实际时钟和单向取消，拒绝“回调阻塞过期但返回ready”的迟到成功。DATA按最多1024B分段，每段和终止符前再核验，取消后不继续下一段或清理协议。

| 副作用进度 | 实际EmailOutcome |
|---|---|
| 配置/鉴权/连接/TLS/AUTH/信封失败，DATA未获354 | NotSubmitted，有限Failure |
| 354后但尚未尝试最终提交许可，取消/到期 | NotSubmitted |
| 最终BeforeSubmit已经开始（含其ACK丢失/拒绝），或正文可能已写；随后掉线/撤销/超时/负面或无效ACK | Unknown，绝不自动重发 |
| 同一次授权和预算内，完整DATA终止后收到严格最终250 | Confirmed{smtp_code:250}：仅本服务器表示接受，不证明手机/最终邮箱收到 |

EmailOutcome是独立类型，没有自动转为当前worker4未带transport的HTTP Confirmed.status。库不自行操作队列；宿主必须保留Unknown和原尝试、拒绝重复claim，不能因重新创建一个sender就重发。关闭通过单向watch true或监督方消失终止本尝试并丢弃真实TLS/socket，不发QUIT/RSET等清理命令、没有脱离调用者的I/O任务。不证明真实Root已杀进程或生产OFF无PID。

## 实际验证与依赖

```sh
cargo test --offline --locked -p rustsimhub-forwarder --test email_sender
cargo test --offline --locked -p rustsimhub-forwarder --all-features
cargo clippy --offline --locked -p rustsimhub-forwarder --all-targets --all-features -- -D warnings
cargo check --offline --locked --workspace --all-targets
```

16个新测试执行真实发送器与实际127.0.0.1 TCP/rustls服务，临时证书、合成账户/地址/正文，无外网SMTP。覆盖最终接受、三种Share/MIME注入、7个阶段逐一拒绝/取消、重复/错claim拒绝、无监督零连接、认证/信封/DATA前后错误、严格多行/累计回复上限、提前ACK、证书链/主机名失败、真实超时/阻塞回调、监督方消失及TLS关闭。夹具Connector只把测试的localhost:465映射到自己实际监听的loopback临时端口，不是生产固定出口的验证。合成EmailFence不冒充真实Core授权。

新增直接依赖`base64 = =0.23.1`，已在原Cargo.lock存在，本包只增加forwarder依赖边，不升级任何包。来源registry+https://github.com/rust-lang/crates.io-index，checksum `ac07cdecf99051d9a5238b80f35af32cdeba5b336e55d957b318b50137e18da5`，包metadata许可MIT OR Apache-2.0；沿用原time0.3.55并开启formatting feature以使用标准RFC2822输出。TLS继续用现有tokio-rustls/rustls/ring栈，测试复用rcgen，未引入完整邮件框架/第二TLS实现。依赖声明不是安全/许可审批，本PR仍需负责人审核。

无新服务/开机启用、生产SMTP账户、真实邮件、设备/ARM/Root/queue迁移、HTTP/IPC接线或发行声明。AGPL-3.0-only，无第三方源码复制；OpenAI Codex辅助实现/审查/验证。
