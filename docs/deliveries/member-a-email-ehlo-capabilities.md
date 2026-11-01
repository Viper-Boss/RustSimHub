# A29：EHLO问候与真实AUTH扩展隔离修正

账号wnfhsj88，2026-11-01；从最新main
`894888561d6a7a921fac95817e10a904812ebf13` 新建个人分支
`codex/wnfhsj88-email-ehlo-capabilities-20261101`。
实现及完整受测源码 `7a76624580de67111250a2639a8c957eb772a5bc`，全包确认运行
tracked_dirty=false、cargo_exit_code=0。之后只补本人报告/索引，最终完整PR HEAD
见PR正文/GitHub回读。不重做已合并PR124的SMTP库，只修这一个真实解析缺陷。

## 触发、旧结果与修正

SMTP EHLO成功回复的第一行是Domain及问候，扩展在随后ehlo-line中。依据
[RFC5321 §4.1.1.1](https://www.rfc-editor.org/rfc/rfc5321#section-4.1.1.1)
的ehlo-ok-rsp语法，单行`250 AUTH PLAIN`可被解释为Domain=AUTH、问候=PLAIN，
不是AUTH扩展广告；续行`250-AUTH PLAIN`同理，不能因后续只有LOGIN/SIZE而
把PLAIN作为已广告能力。

旧`reply`把所有行都按AUTH能力扫描。本轮先加真实TCP/rustls生产发送器回归，
旧代码实际**FAILED**：预期NotSubmitted(Protocol)，却Confirmed(250)，合成本机
服务器观察到完整认证/提交。原失败日志保留，没有放宽断言；这不是已验证的
生产外发事件或其他主机授权绕过，生产SMTP接线仍未启用。

修正只让第一行之后的扩展进入AUTH PLAIN判断，ASCII大小写规则、字节/行数/
总回复预算、TLS验证、逐阶段Fence和DATA副作用边界均保持。三个负面场景直接
执行相同实际模块：单行问候、问候后只广告LOGIN、问候后只广告SIZE，现在均在
BeforeAuthenticate之前关闭，只发EHLO，不发AUTH/MAIL/RCPT/DATA，不重连。

另一正面测试让首行仍含误导性问候、随后有真正大小写混合`aUtH pLaIn LOGIN`
扩展，真实TLS认证和七个原Fence及最终250保持有效。原16测试未删改为宽松结果。

## 实际验证

Linux x86_64，Rust1.98，原`/workspace/toolchains/activate-rustsimhub.sh`。

| 命令 | 真实结果 |
| --- | --- |
| 修复前新专项 `cargo test -p rustsimhub-forwarder --test email_sender --locked ehlo_greeting_is_not_an_auth_plain_extension` | 101，0通过/1失败，Confirmed与NotSubmitted不符 |
| 修复后 `cargo test -p rustsimhub-forwarder --test email_sender --locked` | 0，18通过/0失败/0忽略（16原+2新增测试函数） |
| `cargo test -p rustsimhub-forwarder --all-features --locked` | 子进程退出0，5段共116通过/0失败/1继承忽略 |
| `cargo clippy -p rustsimhub-forwarder --all-targets --all-features --locked -- -D warnings` | 0，严格通过 |
| `cargo check -p rustsimhub-forwarder --no-default-features --locked` | 0，通过 |
| `cargo fmt --all -- --check`、`git diff --check` | 0，通过 |

唯一忽略为原有真实私有tmpfs ENOSPC runner要求，不计通过，没有新增ignore。
本轮只改内部能力解析、真实测试和本人说明，没有公开API/依赖/数据库变化，不为
这个局部变更重跑无关整仓，也不移用PR144/153/157的测试数。远端按本最终HEAD
单独回读，不沿用其他run的成功或失败。

实际日志在`work/a29/`：修复前first-failure、修复后email-tests、全包
package-tests-confirmed与test-exit.json、clippy/production-check/fmt。摘要由本报告
提交时填写，源/源码SHA与测试退出记录直接核对。

| 工作区日志 | SHA256 |
| --- | --- |
| first-failure.log | `753142c6db77d4cd3810a0f65c620605bad3df60bdb57e59624cd5f68af1623c` |
| email-tests.log | `a25edb349d65a3c520fbda0091e0f37c7fadfa0d88712933529f6ca1a298700b` |
| package-tests-confirmed.log | `03694e5d3240c9fddb6258876278202e6d8dcc773399a346c8b111eb47a91eb4` |
| test-exit.json | `e06e0f2df3ff224a91fa4a677d054ea6ef912f09f6930b98b91b4f8d604be982` |
| clippy.log | `56aa4d639afd2762b402c8255abf3175b762225d369af8a2ce222cd0b90ef61c` |
| production-check.log | `9f919280adc8826da556311c37de1e486b86db3a9d17687522612233a83a826c` |
| fmt.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

## 剩余范围与集成

没有改Destination/Edit、outbox3/worker4、Queue/发送worker、Core SMTP Fence、
Root/出口/CSP、默认服务、其他成员或全局台账。只有独立EmailSender库变严；生产
配置、tagged claim/确认、版本及显式兼容/回退仍依PR120具体提案批准后接线。
A29整步和真实获准邮箱/设备验收仍未完成，不能称网页发邮件可用。

可审后合入相同库修正，无DDL/迁移/安装步骤；回退只撤此解析修正并恢复旧行为，
不删除队列/凭据或重发Unknown。实际用户业务/邮箱由主线另行验收。

所有身份、口令、邮件内容、证书和服务端回复均为合成，本轮只连接自己创建的
本机TLS夹具，没有公网SMTP/真实邮件/短信/电话/蜂窝/生产设备/NAS/Root/签名/
安装部署，也不自行合并。AGPL-3.0-only，原作者/许可保留，无第三方代码/依赖
新增；OpenAI Codex辅助复现、实现和验证，无工时估造。已验收81.5%保持。
