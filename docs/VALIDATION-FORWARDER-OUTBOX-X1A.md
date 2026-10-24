# X1A 短信转发规则与持久队列

日期：2026-10-24。源码阶段，不是可启用的短信转发服务。原P0–P7仍79.5%，X1继续单独跟踪。

## 本轮实现

新增`crates/forwarder`原生Rust库，并接现有业务Store/Auth。最近登录真实Owner才能配置；设备密钥、主库realm、Owner密钥及代次共同绑定。配置前后再核对身份，派发检查实际持久Owner、实际短信读取器realm和SIM线路，签名普通客户端不能通过Owner门禁。没有增加网络可调用的未鉴权接口。

以真实已提交的inbox_events为来源：初次/再次启用取当前尾，从下一条开始；停用不读事件，旧短信不隐式补发。删除/恢复事件不是新来信，本地回收站不是转发来源。当前SIM不同的记录、二进制和疑似重复候选不自动转发。同一目标被多个规则命中时取首条，避免重复通知。合法长发送方和正文控制字符与主库范围一致，JSON转义后处理，不能堵住后续短信。

独立SQLite format1与主库schema6分开。启停/改规则增加代次；游标推进和全部目标入队在同一事务内，部分目标或游标写失败则整笔回滚。唯一claim先落盘，才交给未来worker。网络结果不明、Core重启中断或停用后迟到回复保留Unknown，不重新排队。旧Owner/主库/代次、已变SIM和篡改的请求不通过派发核对。

默认通知内容不含号码和正文，完整正文须规则明确选择。当前有Webhook/Bark/Telegram请求编码和有限应答判定：不是实际发送。正文按JSON处理，不启用HTML/Markdown；Telegram使用当前link_preview_options关闭预览，超长元数据前缀被省去时保留完整4096字节正文。凭据类型不实现Debug/Clone，回执不含目标URL、正文或凭据。HTTPS目标拒绝明文、userinfo、换行、片段和零端口；实际TLS/出口选择仍缺。

256条未决、4096条回执、8目标/32规则；队列满不推进游标，Unknown不自动清理，终态清理须由未来所有者接口显式调用。0700目录、0600文件、单链接/非符号链接及独占writer；完整SQLite同步和元数据身份核对。旧数据不能被新的Owner/realm静默采用。

## 验证边界

真实SQLite/Store、签名Auth临时目录测试覆盖：

- 默认关闭、停止的业务actor也不被读取；当前真实事件尾排除历史。
- 重叠规则只排一次，二进制/疑似重复跳过，真正Store换卡阻止旧行派发。
- 实际数据库触发器制造失败，证明多目标及游标原子性。
- 配置失败保留原状态，不假报停机；配置游标不能回退，旧请求不能重放。
- claim崩溃恢复Unknown、迟到结果不改写、错误attempt/篡改内容拒绝、Unknown不被清理。
- 实际Auth签名Owner允许，普通已批准客户端/无效凭证拒绝；真实持久撤权/代次变化和另一业务库拒绝。
- 主库可接受的256字节发送方和正文控制字符不阻塞后续消息。
- 通知脱敏、提供方应答失败不算成功，Telegram完整正文边界和预览关闭。

24项新增转发测试、完整369项Rust测试、全工作区严格Clippy、生产Core无默认特征检查和ARM64库编译通过。命令：cargo test --workspace --all-features --locked --offline；cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings；cargo check -p rustsimhub-core --no-default-features --locked --offline；cargo build -p rustsimhub-forwarder --release --target aarch64-unknown-linux-gnu --locked --offline（均为Rust1.98.0）。这是开发工作树的库编译，没有制造可部署签名包。私有checkpoint：forwarder-outbox-checkpoint-20261024-first.json，最终日志forwarder-workspace-20261024-fourth.log/exit=0；最初导入/SQLite整数读取编译错误已修正，首份失败日志保留。所有输入为合成数据，不是实机转发/用户接收证明。

## 仍必须完成

1. Core真实wire命令、启停配置生产者及独立worker受限IPC，所有者/服务代次贯穿实际进程。
2. 有界HTTPS客户端、证书/主机名校验、固定出口/无蜂窝隐式回退；请求发出前最后一次Core许可及有限取消。
3. Root固定服务主管/worker、停用先持久撤权再停止所有进程及启动触发器，开关/重启/失败状态证明。当前库故意将process_state_verified标false，不能把enabled=false当成无PID。
4. 网页独立开关/规则/回执与合成一键推送，签名安装及真实服务验证。
5. 用户实际目标和已授权规则的真实递送；邮件及后续渠道、正则/时间窗口、明确历史补发和运营商短信转发预算。

没有为本功能访问棒子或开外部推送连接，未安装/启用任何转发服务，未改普通蜂窝数据或关机驱动。现场仍为已交付的7a60650/confirmed46；同期用户已在该版本完成首页只读检查并核对旧操作，另一个有界只读监测等待先前授权的一条10010/CXYE短信，独立记录，不能混作本功能验收。

精确源码ee14f943d93113667e91c8e5695cfd29a4204956的CI40085052613两个任务success，含完整Rust、三个真实材料CLI、13组网页/系统及Native音频。ARM库已核对AArch64与SHA256，未生成安装程序或交付包。同期短信观测已终止且没有新发送，不能将其记为SMS成功。
