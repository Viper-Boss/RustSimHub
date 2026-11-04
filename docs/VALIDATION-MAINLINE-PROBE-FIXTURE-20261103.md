# 主线整合构建：健康探针测试夹具与同源 ARM 材料

## 来源与实际问题

基线为已合并 PR190/194 的 main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`，源码归档 SHA-256 为 `7d5c096f58356529179eaca5ebc300c76be8f04eaa34c40dd4afd7f0e168cbb6`。

这份基线的正式构建在健康探针测试停止：1427 通过、1 失败、11 忽略，尚未开始 ARM 发布构建。失败用例将测试进程本身作为被测应用；实际调试测试程序约 96 MB，超过产品已有的 24 MiB 可执行文件上限，`probe_process` 正确返回 `Invalid`。失败不是设备故障，也不能通过放宽产品上限处理。保留原失败日志，不将缓存中随后重建的同名测试文件当作旧文件证明。

修正只改 `crates/update/src/health/tests.rs`：

- 正向测试启动独立 `/bin/sleep` 子进程，核对真实 PID、start ticks 和实际可执行文件摘要，继续验证不存在 PID 与非法角色拒绝。
- 新增反例：复制真实 ELF 并将尾部扩展到 `MAX_PAYLOAD + 1`，启动后确认探针返回精确的 `Error::Invalid`。
- 子进程由测试独占，退出时 kill/wait；产品上限、生产探针及权限逻辑未修改。

固定测试修正提交：`6d907b3f6a26cac364932b740eba0748f7c58a8f`。完整源码归档 SHA-256：`57cdcd0ec630372ba04661c243a5fadab7c66b97eeda378cb2c0bdf11a5266e2`。后续文档提交不是重新构建的产品来源。

## 本机验证

| 项目 | 实际结果 | 边界 |
|---|---|---|
| 仓库正式脚本 `tools/wsl-build-committed.sh` | 1540 Rust 测试通过、0 失败、11 忽略；12 个默认发布 ARM 程序 | 忽略项没有执行，不计通过 |
| 更新器默认库测试 | 59 通过、0 失败、2 忽略 | 普通 UID、独立仅 loopback 网络 |
| 更新器严格检查 | 全目标/全特性 Clippy `-D warnings` 通过；修改文件 rustfmt 通过 | 不是声称全仓 Clippy 本轮重跑 |
| 原生音频 | 同源默认 ARM 构建、39 项真实 QEMU 测试通过 | 没有运行可选 TURN 实机验收 |
| 正式音频发布 CLI | 实际 ARM/QEMU `--rtc-self-check`，两端接收均 50/50；失效与超时关闭通过 | 仅合成文本/音频流，PCM 未打开 |
| 交付材料 | 13 份 ARM ELF/hash 核对；6 个应用载荷共 22979553 字节，复制回读通过 | 未签名、未分配 epoch、不可安装 |

完整归档逐文件核验，使用固定工具链与既有已验证 target 缓存，没有宣称空缓存或两次独立可复现构建。原生音频 sysroot 逐文件重新核验，没有启动 NAS 容器。第一次原生构建因新增锁定 git 依赖不在离线缓存中停止；随后只运行 `cargo fetch --locked` 补齐缓存，确认锁文件字节保持，再用独立第二次记录完成构建。失败记录保留。

可复核命令：

```sh
# 对上述固定归档执行正式来源核对、全特性测试及默认 ARM 发布构建。
tools/wsl-build-committed.sh <fixed-archive> 6d907b3f6a26cac364932b740eba0748f7c58a8f 57cdcd0ec630372ba04661c243a5fadab7c66b97eeda378cb2c0bdf11a5266e2
cargo +1.98.0 test -p rustsimhub-update --lib --locked --offline
cargo +1.98.0 clippy -p rustsimhub-update --all-targets --all-features --locked --offline -- -D warnings
rustfmt +1.98.0 --edition 2021 --check crates/update/src/health/tests.rs
# 使用固定交叉工具链/sysroot 构建 native/audio-alsa，再用 QEMU 运行实际测试文件及发布 CLI。
```

## 证据索引与剩余边界

原始证据仅在私有目录，仓库不附设备数据、密钥或程序载荷：

- 基线失败：`.private/official-builder-20261103-8f9cfeff.json`。
- 修正后正式构建：`.private/official-builder-20261103-6d907b3f.json`。
- 默认测试/严格检查：`.private/update-probe-integration-20261103-6d907b3f.json`。
- 原生首次缓存失败与第二次通过：`.private/main-native-20261103-6d907b3f.json`、`.private/main-native-20261103-6d907b3f-second.json`。
- 锁定依赖补齐：`.private/native-locked-fetch-20261103-6d907b3f.json`。
- 正式 CLI：`.private/mainline-audio-cli-20261103-6d907b3f.json`。
- 未签名材料：`.private/mainline-unsigned-20261103-6d907b3f.json`。

两个网页版窗口已由用户告知正在执行；不再上传第二份任务书，不修改其接收帧龄、检查运行器或 Root 运行态观测模块。这里只完成主线集成夹具和冻结交付材料；待其 PR 返回后重新整合，不将当前材料当作未来源码的构建证明。

本轮未访问棒子/NAS，未签名、上传或安装。此前固定 7c33324 的签名候选72与本次源码不同，必须分开保留；既有授权不用于给新来源签名。云端 CI、PR 合并和设备验收分别按实际状态记录。下一步先整合外部窗口交付并取得新的只读设备证据，再准备后续候选；人工测试最后。

整体固定里程碑仍 **81.5%**，行动台账 **20/169**、加权 **10.6%**。主线代码约90%仍是未加权粗估；本次修复/测试/构建不直接增加固件验收点。PMIC/关机继续搁置。
