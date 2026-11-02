# D25/26：实际远程 worker 与原 Core 授权接线

2026-11-02；用户批准 #56/#132 的离线实现。起点 #170 `3b1598f2e74fb63064b0f046b1111db2b4c06d1b`，本包在本人分支实现。为保持同一公共native入口只在一个PR，最终追加回同一个 #170，更新其最终范围，不另开重叠入口PR。完整受测源码、命令/计数、产物及失败证据在正式验证后绑定，不能用历史SDK结果冒充新HEAD。

[实现/协议/边界](../remote/worker-v2.md)。仅改明确分配的media授权/远程适配、native audio新独立模块/feature入口及本人夹具/文档；原LAN/v1、正常身份、电话/短信、Root服务与全局进度未改。Native依赖版本/lock不再改变，media仅增加原已有rustix的net feature。来源保留原AGPL及既有SDK上游许可证/作者；AI辅助实施/审查，不改Git作者。

目前实际开发验证已跑通：原Core Authority和协议/Client与真实TURN/DTLS/Opus；错误票据/boot/peer在PCM前拒绝；13类Core绑定撤权关闭PCM；控制EOF、重复id/跨session、超长/未知字段和迟到PCM拥有者保留。另有真实隔离UID984/985/986的三角色实际往返、原Authority撤权、非法UID984/986/987在空闲时拒绝；生产非测试CLI的真实Root拥有合成NSS、固定端点/0660、loopback配置拒绝、985正常解析、SIGTERM原inode清理也实际通过。这些初步命令在脏源码上运行，正式数字另绑。

夹具NSS只在临时镜像，Root控制器只持SETUID/SETGID/CHOWN/FOWNER（无CAP_KILL，清理helper先降到子进程同UID）；三个演员实际CapEff空。read-only/network=none、合成CA/凭据、PCM；未改宿主机NSS、未占生产账户、未安装设备。Core业务Context是合成、Authority与本地授权模块真实，不把它写成完整Core daemon/Owner/电话的验证。

标准test=false目标不会在普通cargo test启动Coturn。新的显式复跑入口 `native/audio-alsa/tests/run_private_remote_worker.py`，另以ignored角色函数在隔离容器按真实UID分别执行；ignore不是跳过后算通过。旧SDK23项仍单独报告，重复源码unit不累加成功能阶段。现有CI因Actions付款/额度未启动，#170 run41325796758及注释已回读；不改workflow或宣称绿。

当前阶段计数仍22/26离线已提交、剩D22/D23/D25/D26四项；固定P0–P7表80%。正式交付时单列可独立提交范围和外部前置，不用测试数量改分母。A生产安全provider/互操作、原DTLS依赖兼容修复/密码审查、正常远程信令与Root登记安装、真实PCM/设备/外网/两端听音保持未验收。

## 正式首轮与续租夹具修正

源码 `d5e5c14c359b9aa406ad3ddd060143c65ba4c44d` 的整仓 `cargo test --workspace --all-features --locked --offline` 实际1446通过/0失败/6项既有专用隔离忽略；严格整仓Clippy通过。原默认Native34、feature42（lib31/bin11）及三种严格native/manual lint和格式均通过。后续修正仅native测试观察/夹具，Root crate源码与lock不变，不重跑或借用其他提交的Root结果。

同源码首次完整私有矩阵**37通过/1失败/1忽略**，不是绿。失败为复用SDK nonce/refresh用例；日志SHA `00e862934499fb21d99edf8c9b5939af37b4800ebf438287e27d27bc95cd7abb`。实际固定库trace显示：收到438后构造新permission重试，但50ms发送cadence仍未到；原用例固定60ms总窗口有机会先推进30s测试时钟，新的请求在实际发送前就被原超时取消，随后正常success被Ignored→Protocol拒绝。初次两轮短idle观察也不足50ms，保持断言失败。

修正仅cfg(test)记录真实STUN请求/响应的opaque transaction id和I/O代次；在既无待响应、又100ms无I/O（超过固定库50ms cadence）后才允许推进协议时钟，等待总预算1秒。等待真实nonce过期时继续调用实际service。保留35×30s推进、真实438、真实Refresh success/后续转发和601s过期拒绝；产品Allocate/Bind/单I/O/凭据/Core/帧期限、原Ignored拒绝和固定依赖不改。临时诊断用直接tracing dev入口已移除，manifest/lock恢复原样，非测试二进制没有新观察字段。修正后该用例真实私有容器单项通过；全套新源码结果在后续正式记录另绑，不以单项替代。
