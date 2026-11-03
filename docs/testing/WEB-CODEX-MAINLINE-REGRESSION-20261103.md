# 网页版 Codex 主线无设备组合回归（2026-11-03）

本交付是开发检查辅助，**不是发行证据、健康评估、生产确认或发布批准**。
只新增 `tools/mainline-checks/` 与本报告。没有改产品 Rust/JS、已有断言、锁文件、
工作流、系统模板、成员归属或全局看板；没有连接设备/NAS/SIM/真实通知目标/NKN 网络，
没有生产签名、部署、迁移、关机、分区写入或自行合并。

## 来源、身份与提交边界

- 任务书：[固定来源](https://github.com/Viper-Boss/RustSimHub/blob/0f375e7c9671014d6eb37f78a756e4a67849b03c/docs/team/WEB-CODEX-MAINLINE-INTEGRATION-20261103.md)。
- fetch 后真实起始 main：`e3762ec9d302a23077825a7db3cc591a2d5dce39`。
- 同仓库分支：`codex/web-mainline-regression-20261103`，不 fork、不直接写 main。
- 首轮受测来源：`9224570b34ac79d1c334916b7f1bf3c27477975b`。
- 最终代码/运行器 SHA、最终受测来源：`7950051b25c1c215a70a4b4e5beeedbedbc33da5`。
- 两轮阶段前后来源均为各自完整 HEAD，Git 来源干净，受测产品代码无未提交改动。
  两轮与起始 main 的产品文件及锁文件一致；差异仅为新运行器文件。
- 本报告在最终测试结束后新增，是唯一后续文件：
  `docs/testing/WEB-CODEX-MAINLINE-REGRESSION-20261103.md`。不把文档提交说成重新执行产品测试。
  包含本报告的最终 PR HEAD 完整 SHA 在 PR 说明回读给出，避免报告自引用自身提交哈希。
- GitHub CLI/连接器实际账号均回读为 `wnfhsj88`；Git 作者沿用环境既有 `GTW` 配置，
  没有改写作者、重写历史、新增团队成员或变更归属。AI 辅助实现和验证，新增代码沿用 AGPL-3.0-only。
- 当前 main 已含更新器灯效资源修复，正常消费；未改写其 indicator、example、驱动或 Rust workflow。

## 环境与依赖

真实宿主为 Debian GNU/Linux 13.6、Linux 6.18.57 x86_64，UID/EUID 1000。
PID1 为 `tail`，有效 Linux capabilities 为 0，无 `sudo`，不能冒称真实 Root、mount/netns、
PID1 systemd/cgroup 或 polkit 集成已运行。没有安装账户、改权限策略或启动业务服务。
初始文件系统可用约 7.5 GiB，复用预置 Cargo/原生库缓存，**不是空缓存构建**。

- Rust `1.98.0 (88d9e12ae 2026-08-18)`；Cargo `1.98.0 (797e8a9bc 2026-08-05)`，
  rustfmt/clippy 已安装，遵循 `rust-toolchain.toml`。
- Node `v24.19.0`；首轮 Python `3.12.14`，最终 Python `3.13.5`。
  后者复用系统既有 `cryptography 43.0.0`，没有给运行器添加第三方 Python 依赖。
- Go `go1.27.3 linux/amd64`；既有 Go 官方归档与预存 go.dev release 清单的 SHA256
  均为 `65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40`，本轮重新计算相符。
- `pkg-config` 实际识别 ALSA `1.2.14`、Opus `1.5.2`。开发库复用环境先前从
  Debian trixie 签名源准备的独立前缀，未替换宿主系统库。
- 依赖来源为已有 Cargo.lock 的 crates.io 依赖、仓库 Go 模块、官方 Rust/Go 与 Debian 包。
  本轮未升级依赖、改源、改安全选项或下载 NKN SDK 候选。C/C++、OpenSSL 沿用现有工具。
- 两轮 Cargo 命令均使用 `--locked`（fmt 除外）；工作区锁文件 SHA256：
  `6615729292791e2518cdd76c663e1cbadc40256b2678c6b4b18d9fd86a5a37da`；
  原生音频锁文件 SHA256：`7da4302fbd8e54a187d3569b4560bbd211804b1d21c009b68b4561529fbbb893`。
- 首轮增量缓存耗尽磁盘后，只移除本会话开始后写入、且不在活动编译器目录中的增量文件
  4,092,393,703 字节；首轮结束后再移除该轮新增增量文件 462,646,595 字节。
  另移除仅首轮生成、未被最终轮覆盖且未被任何活动进程执行的测试可执行文件 1,639,953,720 字节；
  不删库文件、日志或预置产物。最终默认测试完成后还移除该轮 Core/forwarder/update/BLE 的默认测试可执行文件 542,477,208 字节
  （全部特性使用不同特性/哈希，保留所有库、fingerprint、日志与活动程序）。最终轮全部特性命令结束后还移除该轮已执行完、非活动且非失败 Core 二进制的测试程序
  184,574,280 字节，以保留结果写入空间；仍保留全部库和原失败 Core 二进制。五份逐文件名单留在 `.local/`，未清理预置/他人缓存、源码或测试断言。
  最终设置 `CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=4`，保留正常 debug/test 配置和原测试资源限制。

环境实际准备命令（仅对该 shell 生效；Python 链接位于任务 scratch，不修改仓库源码）：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 已建立 /workspace/work/mainline-bin/python3 -> /usr/bin/python3
export PATH="/workspace/work/mainline-bin:$PATH"
export CARGO_INCREMENTAL=0
python3 tools/mainline-checks/run.py --group ordinary --group native-audio --group root --group root-netns --group root-systemd --group runner --output .local/mainline-7950051/report.json
```

## 最终组合结果

最终轮 UTC：`2026-11-02T17:49:29.479325+00:00` 至 `2026-11-02T18:04:51.028215+00:00`（北京时间为 2026-11-03）。
整组退出码 **1**；顶层状态 **failed**，
阶段计数 **passed=52 / failed=3 / not_run=31**。
Root 专用项缺能力使组合不能称为全部通过；浏览器/WASM 另有明确未运行边界。
阶段数和测试数分开，默认/全部特性重复部分不合成独立测试总数。
**最终仍有 Core 恢复和原生音频断言失败，以及兼容构建空间失败；不宣称全绿。**

| 范围 | 真实结果 |
|---|---|
| 全工作区格式 | passed；退出码 0 |
| 全工作区严格 Clippy | passed；退出码 0 |
| 工作区默认特性 | passed; passed=1135, failed=0, ignored=6；退出码 0 |
| 工作区全部特性 | failed；退出码 101 |
| 生产 Core 特性检查 | passed；退出码 0 |
| 独立原生音频 | failed；退出码 101 |
| 原生严格 Clippy | passed；退出码 0 |
| Go 默认离线 | passed; passed=21；退出码 0 |
| 运行器合成单测 | passed; total=20, skipped=0；退出码 0 |
| 固定 Node 网页脚本 | 27 组；{'passed': 27} |
| configurator 合成脚本 | 4 组；{'passed': 4} |
| root | 6 阶段；{'not_run': 6} |
| root-netns | 13 阶段；{'not_run': 13} |
| root-systemd | 10 阶段；{'not_run': 10} |

Rust 默认中 6 项既有 ignored 保持：包括由父测试显式调用的子 worker 标记、真实 tmpfs ENOSPC
及两种 Root 元数据捕获前提；不改 ignored、不增加 mock 来冒充 Root 成功。原生音频只证明
宿主测试/编译，不是 ALSA 物理路由、PCM 通话、ARM 运行或硬件声音验收。
`proc-macro-error2 2.0.1` 的既有 future-incompat 提示保留，当前严格 Clippy 结果如上。

运行器 20 项合成测试单列，只证明真实退出码、信号/中断、跨 session 后代回收、
输出限量、缺工具/能力、0 项选择、未知组、脏来源/中途源码变更、依赖阻断和输出保护。
临时 Git 夹具的 synthetic 作者只存在于临时仓库；不改项目作者或产品断言。
另用真实 TextTestRunner 子进程发现 0 项且退出 0，运行器仍判 failed；两套已安装
Python 的 unittest CLI 零发现均原生退出 5，也判 failed。额外观察脚本最初错误假设
CLI 会退出 0，导致两次观察断言失败，记录留在 `.local/`；仅修正观察方式，没有改产品断言。

## 首轮失败保留及最小复现

首轮 UTC：`2026-11-02T17:35:38.692243+00:00` 至 `2026-11-02T17:47:50.021200+00:00`，真实退出 1，
阶段结果 passed=50 / failed=6 / not_run=30；不把后续环境准备倒写为首轮通过。
首轮使用同样六个 `--group`，输出 `.local/mainline-9224570/report.json`，默认 Python 3.12.14，
`CARGO_BUILD_JOBS=4`，未关闭编译增量缓存。

| 首轮失败阶段 | 退出码 | 最小复现/定位 | 后续处理 |
|---|---:|---|---|
| workspace-all-features | 101 | `cargo test --workspace --all-features --locked`；目标目录所在文件系统耗尽；No space left on device (os error 28) | 保留失败；清理仅本会话新增的非活动增量文件，最终关闭编译增量缓存重跑 |
| production-check | 101 | `cargo check -p rustsimhub-core --no-default-features --locked`；目标目录所在文件系统耗尽；No space left on device (os error 28) | 保留失败；清理仅本会话新增的非活动增量文件，最终关闭编译增量缓存重跑 |
| development-packager | 1 | `python3 tools/test-development-application-packager.py`；默认 Python 3.12 缺 cryptography；packager 父断言报 development packager result differs，AP 两项直接 ModuleNotFoundError | 复用已有系统 Python 3.13/cryptography；运行器新增缺模块前置，缺失时 not_run |
| ap-package | 1 | `python3 tools/test-setup-ap-package.py`；默认 Python 3.12 缺 cryptography；packager 父断言报 development packager result differs，AP 两项直接 ModuleNotFoundError | 复用已有系统 Python 3.13/cryptography；运行器新增缺模块前置，缺失时 not_run |
| ap-install | 1 | `python3 tools/test-setup-ap-install.py`；默认 Python 3.12 缺 cryptography；packager 父断言报 development packager result differs，AP 两项直接 ModuleNotFoundError | 复用已有系统 Python 3.13/cryptography；运行器新增缺模块前置，缺失时 not_run |
| web-recovery-session | 1 | `node tools/test-web-recovery-session.mjs`；tools/test-web-recovery-session.mjs:3 浏览器绝对模块 /wasm/rustsimhub_recovery_session.js 在 Node 无法解析，ERR_MODULE_NOT_FOUND；未执行任何浏览器用例 | 按文件实际用途修正清单；显式保留浏览器/WASM not_run，不用 Node 或 mock 替代 |

空间失败对应 `rustsimhub-forwarder` lib test、`rustsimhub-core` bin test 的临时目录创建，
以及 production-check 的 `target/debug/.fingerprint/` 创建；均为编译器 I/O 错误，
没有修补产品源文件。新发现的产品错误应由负责人分回原作者；本任务没有跨包修复。
清理缓存后没有因“观察超时”重启首轮项；首轮自行退出，最终轮在新运行器/环境下另行执行。

## 最终轮断言失败、环境失败及最小复现

最终全部特性命令原样退出 101，失败即停；截至退出实际 Rust
passed=431 / failed=1 / ignored=2，只是已执行部分，后续工作区包未全部完成。
原生音频 lib 阶段实际 22 passed / 1 failed；worker 等后续测试未跑完。
这两个回归失败不自动归为磁盘问题；根因尚未确定，交负责人分回原模块作者。

Core 原始失败用例：
`network_trial::provision::recovery::tests::partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects`，
定位 `crates/core/src/network_trial/provision/recovery/tests.rs:549:14`：
`control_recovery(...).await.unwrap()` 得到 `Err(Storage)`。
该 Core lib 阶段为 86 passed / 1 failed / 1 ignored，未修改该文件或断言。
临时数据走 `/tmp`（TMPDIR 未设置，观察时约 4.9 GiB 空闲），所以不能直接将 Storage
错误归因于 target 磁盘耗尽；整组可能存在状态/时序影响，尚未作根因结论。

原生音频失败用例 `rtc::tests::encrypted_loopback_roundtrip_and_ticket_identity_revocation`，
定位 `native/audio-alsa/src/rtc.rs:648:22`，`self_check().unwrap()` 得到 `Err(Bounds)`。
这是现有 UDP 合成回环，没有打开真实 PCM、连接外部中继或拨号。

最小源码重建命令（未放宽租约、权限、断言或线程/资源限制）：

```sh
cargo test -p rustsimhub-core --lib --all-features --locked network_trial::provision::recovery::tests::partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects -- --exact --nocapture
cargo test --manifest-path native/audio-alsa/Cargo.toml --locked rtc::tests::encrypted_loopback_roundtrip_and_ticket_identity_revocation -- --exact --nocapture
```

下面实际诊断全部绑定同一完整来源 `7950051b25c1c215a70a4b4e5beeedbedbc33da5`，
直接使用原失败批次已经编译的同一测试二进制，未重新构建或替换它们：

| 诊断 | 实际命令 | 退出码/结果 | UTC 开始→结束 |
|---|---|---|---|
| Core 单项 | `target/debug/deps/rustsimhub_core-899759d94a48167b --exact network_trial::provision::recovery::tests::partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects --nocapture` | 0 / passed; passed=1, failed=0, ignored=0 | 2026-11-02T18:02:54.158351+00:00→2026-11-02T18:02:54.969594+00:00 |
| Core lib 整组 | `target/debug/deps/rustsimhub_core-899759d94a48167b --nocapture` | 101 / failed | 2026-11-02T18:03:27.625131+00:00→2026-11-02T18:03:50.326392+00:00 |
| 原生音频单项 | `native/audio-alsa/target/debug/deps/rustsimhub_audio_alsa-18cc565ce96032cf --exact rtc::tests::encrypted_loopback_roundtrip_and_ticket_identity_revocation --nocapture` | 0 / passed; passed=1, failed=0, ignored=0 | 2026-11-02T18:06:22.861494+00:00→2026-11-02T18:06:25.280311+00:00 |

Core 二进制整组复测 76 passed / 11 failed / 1 ignored：首个失败为
`actual_cancel_restores_without_persist_and_records_both_receipts` 在同一 tests.rs:369 得到
`Err(Storage)`；之后 10 个恢复用例得到 `Err(Busy)`（包括原用例本次在 :536）。
不把它们说成 11 个独立根因，也不把原用例单独通过回填为整组通过。
该诊断与主运行器后续独立检查有时间重叠，时序/负载影响尚未排除；原始 workspace
失败发生在该诊断之前，不能由诊断反向改写。没有继续循环重跑直到取绿。

最终兼容构建退出 101，`rustsimhub-wire`/`rustsimhub-storage`/`rustsimhub-setup-ap`
无法创建 target 临时目录，forwarder 写对象文件报 `No space left on device`。
`upgrade-compatibility` 与 `worker-inspection` 因此保持 not_run；首轮仅
`upgrade-compatibility` 已通过（来源 9224570），不冒充最终 7950051 的通过；
`worker-inspection` 两轮都未运行（首轮错误归入 Root 组，最终已移入普通组但构建依赖失败）。
首轮原生音频 34 项通过同样只属于首轮来源，不能覆盖本轮 Bounds 失败。


## 完整固定阶段记录

下表全部属于最终完整来源 `7950051b25c1c215a70a4b4e5beeedbedbc33da5`，受测源码改动为 false；
实际 cwd 默认为仓库根，仅 Go 为 `tools/nkn-engine`。命令中的 `.local/...` 路径是相同 cwd
下的绝对参数等价表示，未省略检查或过滤参数。时间均为上述 UTC 日期，开始/结束取到秒；
完整 ISO 时间、精确 argv/cwd、进程 PID/启动时间/session、退出码、日志与环境保存在本地 JSON。
`not_run` 项没有伪造子进程/退出码。

| 阶段 / 组 | 准确命令 | UTC 开始→结束 | 退出码 | 结果 |
|---|---|---|---:|---|
| fmt / ordinary | `cargo fmt --all -- --check` | 17:49:29→17:49:31 | 0 | passed |
| clippy / ordinary | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 17:49:31→17:50:30 | 0 | passed |
| workspace-default / ordinary | `cargo test --workspace --locked` | 17:50:30→17:57:12 | 0 | passed; passed=1135, failed=0, ignored=6 |
| workspace-all-features / ordinary | `cargo test --workspace --all-features --locked` | 17:57:12→18:01:51 | 101 | failed |
| production-check / ordinary | `cargo check -p rustsimhub-core --no-default-features --locked` | 18:01:51→18:02:57 | 0 | passed |
| rootfs-syntax / ordinary | `sh -n tools/rootfs/scripts/build-rootfs.sh` | 18:02:57→18:02:57 | 0 | passed |
| recovery-probe-syntax / ordinary | `sh -n tools/recovery-fastboot-probe.sh` | 18:02:57→18:02:57 | 0 | passed |
| committed-build-syntax / ordinary | `sh -n tools/wsl-build-committed.sh` | 18:02:57→18:02:57 | 0 | passed |
| recovery-syntax / ordinary | `sh -n tools/recovery/watch-edl-capture.sh` | 18:02:58→18:02:58 | 0 | passed |
| field-syntax / ordinary | `sh -n tools/field/rsh-readonly-snapshot.sh` | 18:02:58→18:02:58 | 0 | passed |
| rootfs / ordinary | `python3 -m unittest discover -s tools/rootfs/scripts -p test_rootfs.py` | 18:02:58→18:02:59 | 0 | passed; total=36, skipped=0 |
| recovery / ordinary | `python3 -m unittest discover -s tools/recovery -p 'test_*.py'` | 18:02:59→18:02:59 | 0 | passed; total=7, skipped=0 |
| field / ordinary | `python3 -m unittest discover -s tools/field -p 'test_*.py'` | 18:02:59→18:03:03 | 0 | passed; total=5, skipped=0 |
| arm-manifest / ordinary | `python3 tools/test-arm-build-manifest.py` | 18:03:03→18:03:04 | 0 | passed; total=13, skipped=0 |
| pon-review / ordinary | `python3 tools/test-pon-dump-review.py` | 18:03:04→18:03:04 | 0 | passed |
| development-packager / ordinary | `python3 tools/test-development-application-packager.py` | 18:03:04→18:03:05 | 0 | passed |
| ap-package / ordinary | `python3 tools/test-setup-ap-package.py` | 18:03:05→18:03:05 | 0 | passed; total=10, skipped=0 |
| ap-install / ordinary | `python3 tools/test-setup-ap-install.py` | 18:03:05→18:03:06 | 0 | passed; total=10, skipped=0 |
| arm-manifest-optimized / ordinary | `python3 -O tools/test-arm-build-manifest.py` | 18:03:06→18:03:07 | 0 | passed; total=13, skipped=0 |
| web-ap-wifi / ordinary | `node tools/test-web-ap-wifi.mjs` | 18:03:07→18:03:07 | 0 | passed |
| web-call-test / ordinary | `node tools/test-web-call-test.mjs` | 18:03:08→18:03:08 | 0 | passed |
| web-clients / ordinary | `node tools/test-web-clients.mjs` | 18:03:08→18:03:09 | 0 | passed |
| web-configuration-state / ordinary | `node tools/test-web-configuration-state.mjs` | 18:03:09→18:03:09 | 0 | passed |
| web-daily-phone / ordinary | `node tools/test-web-daily-phone.mjs` | 18:03:09→18:03:09 | 0 | passed |
| web-daily-sms / ordinary | `node tools/test-web-daily-sms.mjs` | 18:03:09→18:03:09 | 0 | passed |
| web-deadlines / ordinary | `node tools/test-web-deadlines.mjs` | 18:03:09→18:03:10 | 0 | passed |
| web-diagnostics / ordinary | `node tools/test-web-diagnostics.mjs` | 18:03:10→18:03:10 | 0 | passed |
| web-event-sync / ordinary | `node tools/test-web-event-sync.mjs` | 18:03:10→18:03:10 | 0 | passed |
| web-forwarding-history / ordinary | `node tools/test-web-forwarding-history.mjs` | 18:03:10→18:03:10 | 0 | passed |
| web-forwarding / ordinary | `node tools/test-web-forwarding.mjs` | 18:03:10→18:03:11 | 0 | passed |
| web-incoming-call-test / ordinary | `node tools/test-web-incoming-call-test.mjs` | 18:03:11→18:03:11 | 0 | passed |
| web-indicator / ordinary | `node tools/test-web-indicator.mjs` | 18:03:11→18:03:11 | 0 | passed |
| web-media / ordinary | `node tools/test-web-media.mjs` | 18:03:11→18:03:12 | 0 | passed |
| web-nas-bridge / ordinary | `node tools/test-web-nas-bridge.mjs` | 18:03:12→18:03:12 | 0 | passed |
| web-network / ordinary | `node tools/test-web-network.mjs` | 18:03:12→18:03:12 | 0 | passed |
| web-nkn-status / ordinary | `node tools/test-web-nkn-status.mjs` | 18:03:12→18:03:15 | 0 | passed |
| web-operation-receipt / ordinary | `node tools/test-web-operation-receipt.mjs` | 18:03:16→18:03:16 | 0 | passed |
| web-password / ordinary | `node tools/test-web-password.mjs` | 18:03:16→18:03:21 | 0 | passed |
| web-proof / ordinary | `node tools/test-web-proof.mjs` | 18:03:22→18:03:22 | 0 | passed |
| web-session / ordinary | `node tools/test-web-session.mjs` | 18:03:22→18:03:22 | 0 | passed |
| web-setup / ordinary | `node tools/test-web-setup.mjs` | 18:03:22→18:03:51 | 0 | passed |
| web-sms-management / ordinary | `node tools/test-web-sms-management.mjs` | 18:03:51→18:03:52 | 0 | passed |
| web-test-center-component / ordinary | `node tools/test-web-test-center-component.mjs` | 18:03:52→18:03:52 | 0 | passed |
| web-test-center / ordinary | `node tools/test-web-test-center.mjs` | 18:03:52→18:03:52 | 0 | passed |
| web-usage-stats / ordinary | `node tools/test-web-usage-stats.mjs` | 18:03:52→18:03:52 | 0 | passed |
| web-wifi / ordinary | `node tools/test-web-wifi.mjs` | 18:03:52→18:03:53 | 0 | passed |
| configurator-cancel / ordinary | `node configurator/tests/cancel.mjs` | 18:03:53→18:03:53 | 0 | passed |
| configurator-controller / ordinary | `node configurator/tests/controller.mjs` | 18:03:53→18:03:53 | 0 | passed |
| configurator-gatt-framing / ordinary | `node configurator/tests/gatt-framing.mjs` | 18:03:53→18:03:53 | 0 | passed |
| configurator-static-update / ordinary | `node configurator/tests/static-update.mjs` | 18:03:53→18:03:59 | 0 | passed |
| go-offline-tests / ordinary | `go test -mod=readonly -count=1 -timeout=120s -v ./...` | 18:03:59→18:04:00 | 0 | passed; passed=21 |
| native-test / native-audio | `cargo test --manifest-path native/audio-alsa/Cargo.toml --locked` | 18:04:00→18:04:15 | 101 | failed |
| native-clippy / native-audio | `cargo clippy --manifest-path native/audio-alsa/Cargo.toml --all-targets --locked -- -D warnings` | 18:04:15→18:04:19 | 0 | passed |
| compatibility-build / ordinary | `RUSTSIMHUB_BUILD_SOURCE_SHA256=22af398fe024b691aca6ca63d4e44db78d914c75d8440e94157e3b303006d871 cargo build -p rustsimhub-core -p rustsimhub-forwarder -p rustsimhub-core-launcher --no-default-features --locked` | 18:04:19→18:04:46 | 101 | failed |
| upgrade-compatibility / ordinary | `python3 tools/test-upgrade-compatibility.py --launcher target/debug/rustsimhub-core-launcher --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder` | 18:04:46→18:04:46 | — | not_run; unavailable: dependency: compatibility-build |
| worker-inspection / ordinary | `python3 tools/test-forwarding-inspection.py --worker target/debug/rustsimhub-forwarder` | 18:04:46→18:04:46 | — | not_run; unavailable: dependency: compatibility-build |
| runner-unit / runner | `python3 -m unittest discover -s tools/mainline-checks -p 'test_*.py'` | 18:04:46→18:04:49 | 0 | passed; total=20, skipped=0 |
| board-fixtures / root | `sudo -n python3 -m unittest discover -s tools/system -p 'test_*.py'` | 18:04:50→18:04:50 | — | not_run; unavailable: root, mount, sudo |
| ap-units / root | `sudo -n python3 tools/test-setup-ap-units.py` | 18:04:50→18:04:50 | — | not_run; unavailable: root, sudo |
| ap-build / root | `cargo build -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-setup-ap -p rustsimhub-core-launcher --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root |
| ap-material / root | `sudo -n python3 tools/test-setup-ap-material.py --core target/debug/rustsimhub-core --edge target/debug/rustsimhub-edge --ap target/debug/rustsimhub-setup-ap --edge-user agent` | 18:04:50→18:04:50 | — | not_run; unavailable: root, sudo, dependency: ap-build |
| core-launcher / root | `sudo -n python3 tools/test-core-launcher.py --launcher target/debug/rustsimhub-core-launcher` | 18:04:50→18:04:50 | — | not_run; unavailable: root, sudo, dependency: ap-build |
| ap-chroot / root | `sudo -n python3 tools/test-setup-ap-install-chroot.py --core target/debug/rustsimhub-core --edge target/debug/rustsimhub-edge --ap target/debug/rustsimhub-setup-ap --launcher target/debug/rustsimhub-core-launcher` | 18:04:50→18:04:50 | — | not_run; unavailable: root, sudo, dependency: ap-build |
| forwarder-artifacts / root-netns | `cargo test -p rustsimhub-forwarder --lib --all-features --locked --no-run --message-format=json` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, mount |
| storage-full / root-netns | `sudo -n python3 tools/test-forwarding-storage-full.py --cargo-artifacts .local/mainline-7950051/forwarder-artifacts.json` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, mount, sudo, dependency: forwarder-artifacts |
| update-build / root-netns | `cargo build -p rustsimhub-update --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns |
| update-artifacts / root-netns | `cargo test -p rustsimhub-update --lib --all-features --locked --no-run --message-format=json` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns |
| update-state / root-netns | `sudo -n unshare --net python3 tools/test-update-state-inspection.py --updater target/debug/rustsimhub-update` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: update-build |
| metadata / root-netns | `sudo -n unshare --net setpriv --no-new-privs python3 tools/test-metadata-process.py --cargo-artifacts .local/mainline-7950051/update-artifacts.json` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: update-artifacts |
| metadata-no-kill / root-netns | `sudo -n unshare --net setpriv --bounding-set=-kill --no-new-privs python3 tools/test-metadata-process.py --cargo-artifacts .local/mainline-7950051/update-artifacts.json --missing-kill` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: update-artifacts |
| maintenance / root-netns | `sudo -n unshare --net python3 tools/test-maintenance-observation.py --updater target/debug/rustsimhub-update` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: update-build |
| signed-formats / root-netns | `sudo -n unshare --net python3 tools/test-signed-application-formats.py --updater target/debug/rustsimhub-update` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: update-build |
| ipc-core-build / root-netns | `cargo build -p rustsimhub-core --features dev-fixtures --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns |
| worker-build / root-netns | `cargo build -p rustsimhub-forwarder --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns |
| worker-ipc / root-netns | `sudo -n python3 tools/test-forwarding-worker-ipc.py --core target/debug/rustsimhub-core` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: ipc-core-build |
| worker-process / root-netns | `sudo -n unshare --net python3 tools/test-forwarding-worker-process.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder` | 18:04:50→18:04:50 | — | not_run; unavailable: root, netns, sudo, dependency: ipc-core-build, dependency: worker-build |
| indicator-build / root-systemd | `cargo build -p rustsimhub-update --example indicator-resource --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, mount |
| indicator-resource / root-systemd | `sudo -n python3 tools/test-update-indicator-resource.py --fixture target/debug/examples/indicator-resource` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, mount, sudo, dependency: indicator-build |
| supervisor-core-build / root-systemd | `cargo build -p rustsimhub-core --features dev-fixtures --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns |
| supervisor-worker-build / root-systemd | `cargo build -p rustsimhub-forwarder --features dev-fixtures --locked` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns |
| supervisor-unmanaged / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |
| supervisor-managed / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder --managed` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns, polkit, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |
| supervisor-dns / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder --managed --close-at dns` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns, polkit, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |
| supervisor-tls / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder --managed --close-at tls` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns, polkit, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |
| supervisor-http / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder --managed --close-at http` | 18:04:50→18:04:50 | — | not_run; unavailable: root, systemd, netns, polkit, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |
| supervisor-late_ack / root-systemd | `sudo -n unshare --net python3 tools/test-forwarding-supervisor.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder --managed --close-at late_ack` | 18:04:51→18:04:51 | — | not_run; unavailable: root, systemd, netns, polkit, sudo, dependency: supervisor-core-build, dependency: supervisor-worker-build |

`compatibility-build` 的固定来源环境值来自 `git archive --format=tar HEAD` 的 SHA256，
计算命令/值同时留在 JSON；只供原有限兼容 CLI 使用，不签名或启动业务。
Cargo artifact JSON 由运行器直接保存 stdout，Root 缺能力时不构建并冒称对应真实 fixture 已跑。
清单中普通 worker-inspection 在构建成功时复用 production compatibility-build 的同源 forwarder（其默认特性为空），
不要求本来不必要的 Root/netns。`sh -n` 逐文件执行，避免第二个脚本仅被当成位置参数。

## 未运行、原始记录与交接

- Root、mount/netns、真实 systemd/cgroup、polkit 专用阶段按表 not_run；无 Mock/普通 UID 替代。
  在合适隔离主机上以 EUID 0 启动整个运行器，再选择专用组；只给单个夹具 sudo
  不能保证普通 UID 的运行器有权终止所有 Root 后代，所以能力门禁不将“sudo 存在”视为已运行。
- 浏览器专用 `tools/test-web-recovery-session.mjs` 通过原 `tools/run-web-recovery-session.py`
  及已准备的 native peer/WASM/真实 Chromium 才能形成浏览器证据；本轮为 not_run，
  在 `--list` / `excluded_inventory` 明列，不纳入 27 组 Node 或所选 86 阶段计数。
  不执行该驱动，不改它的安全参数。真实浏览器、IndexedDB/GATT 互通、ARM、物理设备、
  SIM、真实声音和外部网络验收均未完成，不以其他项通过替代。
- 每阶段日志最多 2 MiB，摘要 16 KiB，超时/超限先有界终止并回收原树，不自动重试。
  本轮 JSON、原始日志、环境细节、五次本会话产物清理名单只在 `.local/`，未提交完整环境或私有资料。
  持续报告将未开始的阶段计入 not_run，最终即使可执行检查全过，也不隐去专用组缺失。
- 交付仅将本报告入提交并向 main 开 PR，由负责人审核、决定集成与专用环境接续；不自行合并。
  本轮不提供发布批准或设备验收。整体固定里程碑 **81.5%**、公开动作 **20/169** 保持，
  不因新增工具或测试数改变百分比、分母或成员任务。

