# A-29 / EmailSenderV1：SMTP兼容接口提案（待审）

固定基线：RustSimHub `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。这是配置/queue/worker协议兼容提案与真实现有入口的失败样例，不是SMTP实现。现有EmailSenderV1冻结了Connector/Fence/Claim、三类投递结果和不自动重试的原则，**没有批准SMTP私有格式、迁移或生产派发**。

## 现有实现与最小反例

| 真实源码/符号 | 当前事实 | 邮件扩展必须解决 |
|---|---|---|
| forwarder/src/model.rs `Destination` / `prepare` / `prepare_test` | 只有Webhook/Bark/Telegram；结果都是`PreparedRequest {url,bearer,body,acknowledgement}`，用于HTTPS | 不能把SMTP主机/凭据塞进URL或bearer，也不能将邮件地址放进普通公开配置 |
| forwarder/src/edit.rs `DestinationEdit` / `PolicyEdit::resolve` | Owner编辑与私有持久化类型分开；支持Keep且拒绝未知字段 | 两个枚举都需要明确email变体，Keep不得回显旧邮件账户/地址 |
| model.rs `Policy::view` | 目标只公开id/kind/credentials_redacted | email沿用这三个字段；不返回主机/用户/发件人/收件人 |
| queue.rs `Claim` / `Queue::check_claim` / `claim` | claim的request是现有HTTPS结构，数据库按序列化的精确不可变request核对 | 新通道必须进入真实持久化/claim摘要与Fence；不能凭外置可变配置在发送时改目标 |
| queue/history.rs `fingerprint` / `commit_history_owner` | 历史预览绑定准备好的request，提交重新准备并比对 | transport及完整邮件请求必须参与摘要；不能借历史确认自动增加邮件收件人 |
| delivery.rs `HttpsSender::deliver` | 仅HTTPS，10秒总预算/8192B回复、live Fence及取消；无自动重试 | 不更名冒充通用邮件发送器；单独SMTP实现复用已审固定出口和真实授权 |
| compatibility.rs `runtime_compatibility` | outbox=3，允许从1/2显式迁移，worker protocol=4 | 不能静默写新JSON后继续声称旧版本能读/回退 |
| worker_ipc.rs `parse` / `claim_sha256` | protocol4；Confirmed.status仅检查200..299；摘要包括完整私有claim | 250目前只是HTTP范围内的数值，没有SMTP语义。新确认必须绑定真实claim的transport和协议阶段 |
| core/src/forwarding/ipc.rs `Management::private`中的Check/Finish分支 | 真实Core只允许BeforeConnect→BeforeTls→BeforeSubmit各一次；第3步后NotSubmitted也保守记Unknown | 不能在AUTH/信封阶段反复调用BeforeSubmit；需要协议5的有限SMTP阶段，并保留提交许可ACK丢失的歧义 |
| worker.rs `run` / `Settings` | 私有只读credential、实际低权、固定Route、单前台worker，始终实例化HttpsSender | SMTP派发/关停只能在主线审核后接入，不增加常驻邮件轮询器或隐藏默认路由 |

相对路径均在`crates/`下。对应文件完整SHA256在`source-manifest.json`；清单记录源码截面，不证明运行时授权。

可重跑反例：

```sh
cargo test --offline --locked -p rustsimhub-forwarder --test email_compatibility_proposal
```

测试直接调用当前真实Destination/PolicyEdit/Claim/worker_ipc/HttpsSender：未知email配置、伪装成Webhook的额外SMTP字段、claim中未审transport、SMTP URL/命令注入以及protocol5均被拒绝。实际HTTPS sender在非法SMTP输入下Connector/Fence调用均为0。现有protocol4允许数值250的Confirmed，表明只加一个数值不具备邮件语义；这是不存在SMTP入口的兼容边界，不声称已发现可利用的SMTP漏洞。测试不实现SMTP状态机，也不伪造邮件成功。

## 建议的最小接口变化

以下值仅供本PR审核，尚未分配为正式版本。主线需核对同期版本占用后决定；本包不改任何现行常量、枚举、Cargo或数据库。

1. **私有配置**：新增`Destination::Email`和`DestinationEdit::Email`，严格字段为`kind=email,smtp_host,port,tls,username,password,mail_from,rcpt_to`，未知/重复字段拒绝。只支持一个收件人、ASCII DNS主机、port=465、tls=implicit。所有账户/地址/凭据均使用Secret或同等零化类型；不实现Debug/Clone，不从URL、环境代理、公共JSON或浏览器存储读取账户。现有PolicyEdit仍由真实近期Owner检查，构造/读取/保存配置不启动worker或打开连接。
2. **不可变准备结果**：将`Claim.request`升级为有显式`transport`标签的`PreparedDeliveryV1::{Https {request:PreparedRequest},Smtp {request:PreparedEmailV1}}`；拒绝混合字段。`PreparedEmailV1`包含固定主机/465、账户/凭据、唯一信封发件人/收件人、已经按Share选择并编码好的完整MIME DATA（Secret）；不能到发送时再拼当前短信、改正文、查新的收件人。`Claim`的id/attempt_id/generation/line不变；复用完整claim摘要、精确request相等检查与真实Core Fence。
3. **回执**：保留`DeliveryOutcome::{NotSubmitted,Confirmed,Unknown}`三类。建议Confirmed携带`ConfirmationV1::{Http {status},Smtp {code}}`，Unknown中的已观察结果同样带transport且不含服务端原文。Core按真实claim验证：HTTPS只接受Http 2xx；SMTP只接受Smtp code=250且发送器已完成DATA阶段。不能通过把SMTP250当作HTTP2xx绕过新格式；worker结果本身也不是收件手机的确认。旧公共队列State/Receipt保留，界面文案应为“邮件服务器已接受”。
4. **阶段/Fence**：复用Fence/Claim方法签名，但协议5需追加四个有限Stage：`BeforeSmtpAuth/BeforeSmtpMailFrom/BeforeSmtpRcptTo/BeforeSmtpData`，各自在对应命令前核验。SMTP唯一合法顺序为`BeforeConnect→BeforeTls→BeforeSmtpAuth→BeforeSmtpMailFrom→BeforeSmtpRcptTo→BeforeSmtpData→BeforeSubmit`；最后一步在收到354后、第一段正文写入前执行且仅一次。HTTPS保留原三步。真实Core必须按claim的transport分别选择固定顺序/终态，不开放任意重排、重复阶段或缓存“上一步允许”；这是Core负责人审批/实施范围，本包不修改。Root单向取消通道覆盖整个等待/发送；失败则本地关闭，不再发QUIT/RSET等清理命令。
5. **出口/生命周期**：复用现有Connector及BoundConnector/Route；不提供ambient DNS/代理/蜂窝回退或自行打开socket的替代连接器。默认OFF不启动worker，不产生TLS/DNS/邮件连接；已有独立SMTP库的构造和解析也须无I/O。取消本地future不等于Root已杀进程，真实无PID退出仍需主线核验。

### 有限SMTP与内容子集

- 仅implicit TLS，先验证指定主机名和可信根，成功后才读欢迎/发EHLO或账户。禁止明文、STARTTLS降级、自签名跳过验证、任意IP替换主机名、端口25、EHLO失败后HELO回退。
- 单次`220 → EHLO/250 → AUTH PLAIN/235 → MAIL FROM/250 → RCPT TO/250 → DATA/354 → DATA终止/250`。EHLO需明确广告AUTH PLAIN；只允许一次认证（不处理额外challenge）、一个收件人和一次DATA。不支持PIPELINING/BDAT、附件、任意头、转发多地址、VRFY/EXPN或自动重连。成功最终250后直接本地关闭；不因QUIT失败改变服务器已接受的事实。
- SMTP每行含CRLF最多512B、每个回复最多32行、整个尝试回复合计8192B，回复码/多行续行必须一致且严格解析；超长/截断/非ASCII/非法分隔符/错阶段回复均失败。单一10秒总deadline覆盖所有Fence、连接、TLS、读取与写入；每次等待/返回都核验取消与截止，不能每阶段重新延长10秒。
- 主机ASCII标签、总长<=253；不接IP字面量/URL/路径/尾点。发/收件人每个<=254B，只接受单个ASCII dot-atom local-part@DNS-domain；不接受显示名、注释、引号local-part、逗号、控制字符、CR/LF或国际地址。用户名ASCII可见字符1..64B，密码ASCII可打印字符1..256B且无控制/NUL，保证AUTH PLAIN命令整体不超过512B；不支持SASLprep/其他认证机制，限制需写入实际支持矩阵。
- 不把短信发送方、时间串或正文拼入邮件头。Subject固定ASCII，Message-ID绑定不可变delivery UUID和固定`.invalid`域；Date由受信Core事件/准备时钟生成并一次固定，不能用未校验原始短信时间。正文只由既有Share::Notice/MaskedText/FullText产生，默认Notice；沿用4096B短信输入上限，UTF-8正文用MIME base64/76字符换行，完整编码DATA<=16384B、整个私有准备请求JSON<=65536B。拒绝超过限制，不静默截断、转码或回退FullText。DATA执行点严格实现CRLF与dot transparency，不允许正文控制SMTP会话。
- SMTP服务端banner、EHLO扩展、认证/收件地址、原始错误及通信正文都不进入日志/公开回执；只输出有限阶段/Failure与规则化数值。SASL编码缓冲及MIME/临时JSON同样需零化；不能只零化原密码而保留base64副本。

### 副作用边界与失败映射

| 实际进度 | 结果 | 重试/含义 |
|---|---|---|
| 校验失败/关闭/撤销，或连接、TLS、AUTH、MAIL/RCPT拒绝，且未尝试写DATA正文 | NotSubmitted + 固定Failure | 没有提交正文；仍不自动重试/重发 |
| DATA获得354，但还没进入正文write；此时取消/超时 | NotSubmitted | 不发送正文/终止符，不继续清理协议 |
| Core已发出BeforeSubmit许可，或其ACK丢失；即便worker自报还没write | Unknown | 沿用真实Core的保守映射，不凭worker自报撤销可能已经使用的许可 |
| **第一次正文write之前先设attempted=true**；其后部分写/断线/超时/取消/非法或4xx/5xx最终回复 | Unknown | 即便write报错也不能证明远端未收到；保守不重试，不把“关闭”改成未提交 |
| 完整DATA终止后，在同一连接/尝试、期限与授权仍有效时收到严格最终250 | Confirmed(Smtp 250) | 只证明该邮件服务器接受；不证明最终邮箱/手机接收 |
| 250仅出现在欢迎/EHLO/MAIL/RCPT或内容中，或迟到/旧claim回复 | 不构成确认 | 按真实阶段和attempted处理；不解析错误正文猜成功 |

以上为保守的副作用边界：Unknown不会被再次claim或自动迁移为Queued；重新开启/重启/换配置也不能重新发送它。

## 数据与版本兼容决策

建议新writer outbox **4** / worker protocol **5**；这两个数字是提案，不在本包生效。现行runtime和实际数据库继续为3/4。

| 数据/程序组合 | 提议行为 |
|---|---|
| 旧3数据库 + 当前3/worker4 | 完全保持当前行为；不因本包触发迁移 |
| 旧3数据库 + 未来4/worker5 | 先只读验证actual application ID/版本及Root/data/update三方兼容；由主线明确授权维护窗口与可读备份，再事务迁移 |
| 未来4数据库 + 旧3/worker4 | 必须拒绝；不谎称回退兼容，不自动删email目标或取掉transport标签 |
| worker4与未来Core5，或反向 | 拒绝protocol，不协商降级/用HTTP结果冒充SMTP |
| 未知application ID/版本/坏JSON/额外字段/混合transport | 拒绝且保持源库，不清库、不修补为默认成功 |

迁移须保留realm/Owner绑定、generation、事件游标、request幂等ID、delivery/attempt ID、去重约束和全部历史状态；仅将真实旧HTTPS准备结果封装到Https变体。**不重算/重新准备正文、不增加收件人、不重新签名claim、不自动开启功能或重放queued/Unknown。** 旧in-flight需先停止并证明唯一worker退出；不确定的attempt保持Unknown。Prepared历史预览因摘要字节变化应显式失效并要求用户重新预览，不能无声复用旧确认。迁移的物理schema、哈希版本域及历史plan转换由queue负责人给出精确diff并审批，本PR不擅自实现。

更新器必须在暂存前比较候选/当前/回退对数据4的支持；没有三方支持就不能装载会写4的候选。若要先交不持久化的SMTP独立库，其测试只用内存合成请求，不连接旧queue，也不能声称生产邮件可用。

## 依赖提案与实现顺序

SMTP命令/有限回复解析用Rust标准库及既有tokio/rustls/zeroize，不引入完整邮件框架或新TLS栈。MIME/SASL base64拟新增forwarder的直接`base64 = "=0.23.1"`依赖（本包不改Cargo）；它已存在当前Cargo.lock，但作为新直接依赖仍需审批。锁定来源registry+https://github.com/rust-lang/crates.io-index，checksum `ac07cdecf99051d9a5238b80f35af32cdeba5b336e55d957b318b50137e18da5`，metadata声明MIT OR Apache-2.0。既有tokio1.53.2(MIT)、tokio-rustls0.26.6(MIT OR Apache-2.0)、rustls0.23.46(Apache-2.0 OR ISC OR MIT)、zeroize1.9.1(Apache-2.0 OR MIT)继续锁定；离线metadata及校验和不是漏洞审查或许可审批。

Date格式化若需启用现有time的formatting feature也须在实现PR列出精确Cargo/lock差异。不得以“已有传递依赖”省略新增直接使用的许可/来源检查，不批量升级锁文件。测试TLS继续使用现有rcgen/临时本地证书，不复用生产密钥。

负责人需要在本具体提案上决定：SMTP子集/固定465、私有字段和Share/Date来源、tagged claim/确认、版本号与迁移/回退规则、新直接依赖及有限SMTP Fence阶段。审核后才在`crates/forwarder/src/email/`实现独立SMTP/TLS库及真实loopback TLS服务，按上述每一阶段注入拒绝/超时/证书错误/撤销/关闭/掉线；后续最小生产派发与queue迁移另外精确审核。不发送真实邮件、不访问外网SMTP、不安装服务，不改Core/Root/queue/worker或全局完成率。

沿用AGPL-3.0-only，无第三方源码复制；OpenAI Codex辅助提案及当前接口失败样例。
