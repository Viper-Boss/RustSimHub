# A-29：真实SMTP/TLS独立实现与本机邮件夹具

账号wnfhsj88；主线起点80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d；实际实现/受测完整HEAD：017164175759088c2e8167d56a850c5f663b52b2。后续仅本交付记录，最终PR HEAD见GitHub。用户要求继续实际编码并以PR提交；兼容提案PR120仍在审，当前生产协议/队列不变。

新增`crates/forwarder/src/email/mod.rs`及`tests/email_sender.rs`，通过Linux pub mod公开独立候选，文档`docs/email/smtp-sender.md`。修改forwarder Cargo与lock最小依赖边：直接复用原锁定base640.23.1，time0.3.55开启formatting；没有新增锁定包/升级版本或第二TLS栈。依赖来源/完整校验和/许可见说明，声明不等于审批。

## 实际行为

真实implicit TLS/465、证书链/主机名验证、AUTH PLAIN、单收件人、有限SMTP/MIME/base64。复用实际Connector、Share/Message/masked_text、Secret/zeroize及现有rustls。7个独立EmailFence阶段检查完整不可变claim摘要；默认不提供实际Core适配。动态撤销/监督方失联、每次回调后与每段写前的截止/取消复核、回复512B行/32行/累计8192B、单一10秒预算均真实实现。

最终DATA许可可能已下达后，掉线/超时/取消/负ACK均Unknown；只在完整终止后收到同次有效最终250才Confirm本服务器接受。独立EmailOutcome不自动转为worker4的无transport HTTP回执。无自动重试、重连、QUIT/RSET清理、后台任务或常驻服务；所有私有配置/通信临时缓冲不输出日志。

## 对应实现HEAD真实验证

激活工作区Rust1.98：

- `cargo test --offline --locked -p rustsimhub-forwarder --test email_sender`：16项通过；真实127.0.0.1 TCP/rustls服务、临时证书与合成账户/正文。
- `cargo test --offline --locked -p rustsimhub-forwarder --all-features`：**109通过、0失败、1忽略**。忽略真实ENOSPC tmpfs测试保持未运行，不计通过；旧93项+新16项实际执行。
- `cargo clippy --offline --locked -p rustsimhub-forwarder --all-targets --all-features -- -D warnings`通过。
- `cargo check --offline --locked --workspace --all-targets`、fmt/diff通过。整仓本包执行的是编译检查，不借用统计分支856等测试数字；已有proc-macro-error2未来兼容提示未修复。

测试覆盖三个Share与命令/头注入、精确私有fingerprint、7阶段逐一拒绝/撤销、重复/错claim拒绝、关闭/监督方缺失零连接、认证/信封拒绝、DATA前/后掉线及最终负ACK、行/多行/跨阶段累计回复上限、提前ACK、证书/主机名错误、真实超时与阻塞回调、监督方失联后真实TLS/socket关闭。外部权限端口为明确合成夹具，未复制发送器或冒充Core；发送器使用的是实际生产模块。

日志在setup-logs的`member-a-smtp-{tests,package,clippy,workspace-check}.log`。未提交私有账户、真实短信、设备日志或二进制。

## 仍未验收

真正Owner/Core EmailFence、tagged claim/确认、Destination/PolicyEdit/queue迁移、worker新协议及Root启停未实现/安装，须按PR120精确审核后由相关模块负责人集成。当前Core只有原3阶段，各一次，不能伪造适配或复用BeforeSubmit授权全部SMTP阶段。独立构造元数据不提供权限，库的单次尝试也不是持久防重队列。

没有真实外网SMTP/邮件/获准邮箱测试、ARM/设备/Root或真实生产OFF无PID；本机夹具不等于固定生产出口已核验。这里只完成A-29独立发送器和离线TLS失败测试，不将邮件整体功能标为验收完成或修改全局进度。下一步继续NKN进程/构建候选，生产邮件接线留待具体审核。

AGPL-3.0-only，无第三方源码复制。OpenAI Codex辅助实现、审查与验证，历史作者保持。
