# A-29：邮件转发兼容提案与失败样例

提交账号wnfhsj88，按用户指示接手成员A。起始主线完整SHA：80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d。提案/测试完整HEAD：472358755db3671a2b16d6d142b0a074d7848b06；后续仅增加本交付记录，最终PR HEAD见GitHub。

交付范围为`docs/email/`、`crates/forwarder/tests/email_compatibility_proposal.rs`及本报告。未改forwarder生产类型/队列/worker、Core、Root、Cargo/lock或当前outbox3/worker4。提案描述建议值outbox4/worker5，不代表已批准或已实现。

对照12份真实公开源文件，逐个以git show比对固定主线，再记录完整Git blob、SHA256和字节数。确认现有配置/准备请求只有HTTPS；协议4数值250在HTTP2xx范围内，不能证明SMTP完成。提出私有配置、显式transport请求/确认、完整claim摘要绑定、有限SMTP/TLS子集、DATA副作用边界与三方更新/回退兼容方案。配置/版本/迁移/依赖必须先由负责人审核，符合任务书要求。

对应上述HEAD实际执行（激活工作区工具链）：

- `cargo test --offline --locked -p rustsimhub-forwarder --test email_compatibility_proposal`：4通过，0失败/忽略。直接调用真实公开配置/claim/IPC/HTTPS sender，未知SMTP配置/字段/协议及命令注入被拒；实际sender的Connector/Fence调用为0。不打开网络socket、不编造SMTP状态机。
- `cargo clippy --offline --locked -p rustsimhub-forwarder --test email_compatibility_proposal -- -D warnings`通过。
- `cargo fmt --all --check`、`git diff --check`通过。
- `cargo metadata --offline --locked --format-version 1`完成，核对当前锁定base64/既有TLS依赖版本与metadata许可；不是漏洞/许可审核通过。

日志为工作区setup-logs中的`member-a-email-compat-{tests,clippy}.log`和`member-a-email-metadata.json`，不提交原始环境信息。未为仅增加测试/文档重跑整仓或借用旧PR的测试数字。

A-29实现仍待批准后开展：独立SMTP/TLS库、真实本机TLS邮件夹具、生产派发、queue迁移、OFF无进程及获准邮箱实测均未完成。没有真实邮件、外网SMTP、设备操作或发布；不把提案计成邮件功能完成。下一步按具体提案审核意见实现独立邮件库，生产共享入口另行精确审核。

AGPL-3.0-only，无第三方源码复制或新增依赖；OpenAI Codex辅助实现失败样例与提案，不改历史作者或全局验收进度。
