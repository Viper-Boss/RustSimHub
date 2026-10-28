# D-13～16：真实指示灯输入与有限语料验真

日期：2026-10-28。执行者：Viper-Boss / Codex。范围是离线实现、测试与审计，不是设备灯色或整个 LED-07 / SEC-06 的验收。

最新兼容截面：合并Claude main `dfd11e9e75d7b532de5fcaeacbdb9fcd87e33215` 后，按用户新配色更新离线断言；Core170+Store63+灯控24=**257项**、严格Clippy/fmt、Python进度23及Node看板一致性通过。源码归档 `3df8c717ea5b2569071dffca6724062015ec9679d050b9e3fb1d9ffa7162b98d`，日志 `97e66fb6be1a77a8adb61281554f5370a8a5577fe9b9e080005a3c050aec65ad`。同一次三方合并保留Claude的UPD-04/STAB-03和本分支SMS-05，台账20/169、10.6%；下表256项是旧main截面证据，不冒充最新。

## 源码与信任边界

生产代码截面 `6078a3766361cacfe4d7d059867a27ae4ed7881b`，后续合并 Claude 的 `3d7f8488220d73db6b71e45c40087d00f5a507e7` 仅涉及文档。实际入口：

- `crates/status-led/src/lib.rs`：颜色、优先级和节奏的纯策略。
- `crates/status-led/src/linux.rs`：固定运行路径、文件权限/归属、服务/网络状态、更新记录、实际 LED 输出。输入不是任意 sysfs 路径。
- `crates/core/src/status_led.rs`：业务提示文件、未读确认持久账本、配置视图租约。
- `crates/storage/src/lib.rs` 的指示灯快照查询：当前线路、未删除短信和未接电话；不把旧卡记录当当前未读。
- 新测试文件为 `status-led/tests/offline_policy.rs`、`status-led/src/linux_input_boundary_tests.rs`、`core/src/status_led_input_tests.rs`、`storage/src/offline_input_boundary_tests.rs`；原模块仅增加 cfg(test) 接入。

业务提示目录固定为 Core 所有、组 root、2750，文件为0640；灯控检查文件归属、链接数、类型、长度和 JSON。测试使用执行测试的 UID/GID，不能替代真实 UID 986 / 无 capabilities 服务运行检查。更新记录由 root 固定目录0700、文件0600提供；灯控只分类记录，签名验证属于上游更新器，不能由“有 staged 文件”推断验签通过。

策略实情：关闭→熄灭；来电响铃白色200/200毫秒优先；错误红色；升级紫色慢亮灭；配置白色慢亮灭。首次6078a37截面按旧未读配色验证；随后同步Claude已合入main的dfd11e9（用户2026-10-28改定），当前短信黄/紫/黄/紫、未接红/紫/红/紫各1.2秒；都有时红/黄/紫/红/黄/紫1.8秒。各色280毫秒、间隔20毫秒，每30秒重播。离线断言同步新决定，不改回生产旧配色。可用更新紫色200毫秒/10秒；I/O青色120毫秒且至少间隔5秒；蜂窝优先绿色，单 Wi-Fi 蓝色，无互联网黄色。max_brightness=1 的硬件只能慢亮灭，不宣称平滑 PWM 呼吸。

## 已复现并修复：FIFO 先阻塞，后验类型来不及执行

Core 账本读取和灯控输入读取均先用 `O_NOFOLLOW` 打开，再用打开文件的 metadata 检查普通文件。FIFO 在打开阶段就可能等待写端，使后面的类型检查永远不到达。

两个父测试分别建立临时 FIFO，启动真正的测试二进制子进程调用实际私有读取函数。原实现均在2秒截止仍未返回，父测试 kill 并 wait 后失败；不让卡住的进程遗留。修复仅在打开标志加入 `O_NONBLOCK`，仍保留 nofollow、实际 metadata、归属/权限/链接/长度检查。普通文件语义不变，FIFO 会及时被类型检查拒绝；没有新增任意路径产品接口。

| 证据 | 源码归档 SHA-256 | 日志 SHA-256 | 真实结果 |
|---|---|---|---|
| Core 修复前 | `46745728788ad2847a6cba0b94f00cbbaeadc935460cb1b5386156363f0134ff` | `9a2fc592e824997431835bce568a4a2801ce249f2b6065901a438345578af07d` | FIFO 子进程超时，父测试失败 |
| 灯控修复前 | `c457b688c97e4c18173ca4cc31e52ebaba03c9fad5af21492a43c16ff9d034bd` | `bddd21b4275d0dd18d776fdc9c2b71a8cf259b83631a681e24298506c8ac655b` | FIFO 子进程超时，父测试失败 |
| 最终修复截面 | `f9be9c74a3ca7f543a6c41f9b7af7d1ea061394e5189cc63e7c409b65f512f93` | `6a33a43617dc6c0fea0189fac79e0e72ac8bf4c6b7d4c45cd6e2610ed20d4110` | 256测试、严格 Clippy/fmt、23进度测试通过 |

首次独立灯控夹具错误使用未启用的 rustix::process，编译失败；改为从测试文件 metadata 读取本用户 UID，未增加生产依赖。首次修复后256测试通过，但严格 Clippy 拒绝一处测试断言的无谓 clone；改用 `std::slice::from_ref` 后才得到上表最终全绿。前次失败日志保留在私有证据目录，不将“测试绿、lint红”写成完整通过。

## 矩阵与可复现命令

- 7个公开策略测试：256种低优先输入 × 6个时间点验证优先级，颜色/网络组合、两种通知顺序及确认恢复、配置覆盖、稀疏 I/O/升级提示预算。
- 实际 Linux 文件读取：FIFO、目录、软/硬链接、权限、归属、超长/截断 JSON；Core 实际持久账本的错字段、错类型、无效 UTF-8、容量与文件长度边界。
- 固定版本语料 `tools/input-corpus/sms-calls/v1.json`：16个短信、10个通话，共26个合成向量；真实 Store 接受12、拒绝14，逆序重放不增事件，重开仍稳定，所有控制操作表保持空。静态 seed 是版本标签，不声称随机模糊测试。
- 实际 Store 指示灯快照：删除/恢复、调用不可用、换 SIM、旧线路迟到；证明数据库当前快照的隔离。

```sh
cargo test -p rustsimhub-storage -p rustsimhub-core -p rustsimhub-status-led --all-features --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-core -p rustsimhub-status-led --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s tools/progress -p 'test_*.py'
python3 tools/progress/build.py --check
```

隔离 Linux / Rust1.98，实际任务 UID65534、无 capabilities、无网络/设备、源码和依赖缓存只读、临时数据库/文件；256项=Core170+Store63+灯控库10/程序6/集成7，0失败/忽略。新增15个测试入口中2个仅为 FIFO 子入口，不能算15项功能。没有操作棒子、签名、部署、通话、短信或流量。

## 必须保留的缺口

1. 业务提示只有 expires/configuring/unread_sms/missed_calls/ringing，**没有 SIM epoch**。旧卡已写出的提示可能在换卡后保持到下一次更新/过期。Core 生成10秒租约，读取器接受上限30秒，200毫秒读缓存、Core每秒更新；本次证明 Store 查询隔离，不能宣称跨进程瞬间撤销。应另提版本/代次兼容方案，再改生产格式。
2. Core 配置视图租约25秒、最多16个，绑定 principal 和 view；使用实际 Instant 和固定路径，本次没有提供任意路径测试接口，也没有用重新实现一套方法冒充实际配置发布测试。
3. 未测试真实 sysfs、GPIO混色、热插拔、重启/服务权限、真实更新安装与设备通知。现有设备记录归 Claude 的独立交付；不能算本批验证。
4. D-15 语料覆盖 Store 入口和实际内容预算，没有新增 MM 原始 PDU 解析器，也没有覆盖所有未来 BLE/媒体解析。LED-07、SEC-06 保持未闭环；后续 D-26 补新增模块矩阵。

本批 D-13～16 离线任务交 lead，待精确 PR HEAD 审核；全局功能完成数不因转移四步增加。
