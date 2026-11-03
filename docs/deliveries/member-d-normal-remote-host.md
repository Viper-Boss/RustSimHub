# D25：正常 Core 身份/租约到实际远程 worker

后续2026-11-03：[固定安全候选闭环交付](member-d-secure-recovery-integration-20261103.md)已补齐D22/D23本人离线实现与验证，累计26/26离线提交、独立离线剩0；本文保留原24/26来源截面。候选冻结/生产准入/安装/设备验收和固定80%独立保留。

2026-11-02。用户已批准 #56/#132 离线实施。沿用本人 PR #170，起点 `a333a1841137e105348fa4c402de6ec122aa0691`。本包完成 D25 正常业务门禁及强制中继的离线实现交付：**24/26 离线阶段已提交，剩 D22/D23；固定 PROGRESS-SCALE 仍 80%**。提交数不代表主线审核、生产密码准入或设备验收；不更新全局台账。

完整受测源码：Root/真实 x86 三角色运行 `198597740bb11b39e8563c4e02698c7a5de0d4f1`；Native 标准回归、取消负例及 ARM `fc33681c9ad43cbdfab0247dac94379299ff973d`。两者生产 Rust、Root 测试、manifests/locks 相同，后一提交仅把 Native 测试辅助模块移入 fixtures、更新路径及说明。最终包装提交仅更新交付文档/机器证据，最终推送 HEAD 以 PR 回读为准，不能把旧运行冒充新增 HEAD 重跑。

[实现和接口](../remote/normal-signaling-host.md)、[逐来源机器证据及日志 SHA256](../remote/member-d-normal-host-evidence.json)、[依赖闭包](../remote/turn-native-dependencies.json)、[复跑说明](../../native/audio-alsa/tests/fixtures/README.remote-worker.md)。

正常信令 Host 首先调用原 Core CheckMediaLease，身份、scope、活动呼叫和原租约通过后才连接 Media/准备 TURN。实际 Verified peer 成立后才 IssueMedia/Authorize。固定 UID/端点、SO_PEERCRED、版本/id/对象形状均核对，每步再次核 Core binding；Core await 取消也关闭同一 worker fd，激活尝试不可自动重发。只输出脱敏状态，没有第二 Auth/Call/Lease 库、任意代理/电话 RPC、续租/挂断或新 Owner。

范围：media 显式 remote-signaling 模块、原 Client 取消围栏、Native turn-relay 接线及测试；Core 只增加测试 feature/hook，不改变生产 Auth/Calls/media_ipc。Root lock 只追加已有 workspace 边；Native lock 189→192，仅增加本仓 domain/policy/wire。活跃依赖149→152，145个 registry 包版本不变，OSV 查询沿用证据中注明时间的原快照，没有声称重新联网扫描。原 DTLS RUSTSEC-2026-0387 保持，production_admitted=false。

正式验证（Rust1.98，CARGO_INCREMENTAL=0，locked/offline）：

- `cargo test --workspace --all-features --locked --offline`：**1455/0/7**，61 个 suite；新增 Core 角色专用 ignored 入口在实际跨 UID 场景另执行，其余6项不声称执行。
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`、默认 Core no-default-features check：通过。既有 proc-macro-error2 future compatibility 提示保留。
- Native 默认 **34/0/0**、turn-relay **42/0/0**；Native all-targets、两个显式 manual 目标严格 Clippy、Root/Native fmt、diff：通过。
- 实际原 Core 登录/Store/Calls Controller/normal dispatcher/media_ipc 的5个边界函数，及正常 Core 端口2个解析/来源函数通过。后2个因源码复用编译两次，不把9次执行称9个独立新函数。
- x86 实际 TLS/Coturn/ICE/DTLS/Opus worker 矩阵 **38/0/2**；38包含23个 SDK函数、5个源码内 unit、10个 worker integration，不与标准 suite/其他架构重复相加。2个角色入口均由专用夹具另执行。
- 实际 UID986 原 Core、UID985 非测试 library Host、UID984 实际 worker：正常身份/活动呼叫/原租约、实际媒体票据消费、双向 Opus、原 Owner 撤权关闭真实媒体均通过。原假 ModemManager creates=1/starts=1，仅私有 D-Bus 计数；Host 未续电话租约。旧三角色/非法 UID 及非测试 CLI 夹具也通过。
- `replay_normal_host_negative.py`：仅在干净 archive 移除 Host.poll 的外层取消围栏，原 Core 取消/EOF 断言保持，按预期 exit101/1 failed/Elapsed；原树不变。这是人为故障注入，不是历史生产漏洞声明。
- AArch64 release 实际交叉构建：**9,255,720 字节**，ELF SHA256 `c49fadd1484afc6ba830f223a6cf545e746a8385024edf0a27d9a06a8b45df0d`。QEMU Cortex-A53 单 worker 矩阵 **38/0/2**，128MiB/16pids/2CPU、UID1000/caps空、network none/read-only。
- ARM Host + ARM worker（各实际 UID985/984、cap0），接原 x86 Core（UID986/cap0），真实 TURN/双向 Opus/Owner 撤权场景通过。组合容器 **128MiB/24pids/2CPU**；Core 不是 ARM，容器不是整板/单服务预算证明。

失败如实保留：

- 1985977 首轮 Native 默认构建因 tests 顶层辅助文件被 Cargo 自动发现为独立目标而编译失败；仅移入 fixtures/更新路径，fc33681 完整 Native 重验通过。没有扩大默认 feature 或加 ignore 掩盖。
- ARM 两个 QEMU 演员 + Core/Coturn 组合在16pids下启动失败，保留 Coturn `Cannot create cli thread: Resource temporarily unavailable`；仅该组合夹具改24pids后通过，产品配置/授权期限不变。
- ARM worker 矩阵与另一 QEMU 容器重叠运行时，既有 self_check 返回 Bounds，37/1/2；未定位具体 Bounds 分支。同一 ELF/128MiB/16pids/2CPU 独立串行重跑38/0/2。原失败与日志 hash 保留，没有放宽帧龄/期限/断言，也不据重跑声称并发负载稳定性验收。

NSS 只存在临时镜像，Root 控制器仅容器 SETUID/SETGID/CHOWN/FOWNER，无 CAP_KILL，实际演员 cap0。仅替代外部私有 ModemManager、PCM、TURN 登记/CA 和远端音频对端；原 Core 身份/业务 Controller/Authority 不再用合成 Context 代替。没有 host 账户/设备更改、真实电话/短信/APN/外网 TURN、生产签名或部署。保留 AGPL/原作者及上游许可证，Codex 辅助，未知实耗不编造。

剩余 D22/D23 需要 A 的生产安全会话、pin/签名域、Verifier/Sealer。已只读回查 main `1e5cadd9c32c6576e77f10c9e65f0eab87076098`：A-27 仍 production_ready=false/crypto_wire_frozen=false，不能用测试替身接生产。DTLS兼容修复/密码审查、正常消费端路由和 C/Root 正式登记/安装、实机/外网/两端听音继续由对应成员/主线收口。未自合并或修改其他成员/全局台账。
