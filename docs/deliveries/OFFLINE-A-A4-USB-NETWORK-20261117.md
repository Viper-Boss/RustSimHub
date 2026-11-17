# A4：USB 共享配网、计量与网页复验（2026-11-17）

后续更新：本报告保留首批固定HEAD的原始边界；隐藏SSID的Core实现与新增验证见[后续交付](OFFLINE-A-A4-HIDDEN-USB-20261117.md)，外部工具与现场仍未验收。

状态：本组离线修复可供审核；A4 全部验收尚未完成。整体固定里程碑仍为 **81.5%**，不修改全局台账或权重。

- 起点：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
- 实现 HEAD：`e6ec6b545bbc3f39d7b376b06be497b5e057446e`；报告提交在其后，最终报告提交 HEAD 以 PR 的当前 head 为准，避免自引用。
- 实现树：`f4698cbf934e16f934b51247dedb62c13ac96241`。
- 固定源码 tar SHA256：`15f17c3e5307d406b2568f4d9795bbaee3fe50d41a36cd52411ffed1f3bf12a2`。
- 分支：`codex/offline-a4-usb-network-20261117`。未自审核、合并或部署。

## 实际修复

1. `crates/network-agent/src/linux.rs` 与 `linux_tests.rs`：计数基线改用 Option。首样本/计数回退/零时间间隔仍未知；真实零基线、空闲速率为可证明的零。避免以 `(0,0)` 哨兵把真实数据永久显示未知；不再把零时间强行夹到 1ms。未改变限额、APN、计费或 JSON 字段契约。
2. `crates/core/src/network_trial/provision.rs`：普通管理授权在既有 `spawn_blocking` 中等待 Auth 锁，再检查当前授权。其他客户端的并发鉴权查询不再因 `try_lock` 的 Busy 提前结束合法试连。原 principal 比较、NetworkManage、NAS 拒绝和撤权检查保留；未修改 recovery 子模块或共享协调器。
3. `provision/tests.rs` 与 `provision/tests/a4_usb_nm.rs`：显式 ignored 的真实 Core 授权入口与 NmProvision 适配测试，只有显式私有 D-Bus/网络隔离环境才运行。模拟 NM/MM 只提供固定方法，不调用主机系统总线、扫描、热点、蜂窝或真实设备。

代码沿用仓库 AGPL-3.0-only；新增测试同许可证，无第三方源码复制，无接口或数据库迁移。

## 测试及负对照

固定 Rust 1.98.0；Ubuntu-24.04 WSL2；Cargo `--locked --offline`。本地原始日志、完整命令与 SHA256 随测试包保留。

| 检查 | 实际结果 / 边界 |
|---|---|
| 速率负对照 | 保持旧逻辑语义的 helper 提取，两个新增测试均实际运行 1 项并以 101 失败；不是未经改动的 main |
| 速率修复 | 同两个用例均 0；网络/灯控 Cargo 回归合计 63 项通过 |
| USB 负对照 | 修改 Auth 锁前，精确库测试实际运行 1 项，save 回退且无 NM 写副作用，退出 101；保留日志 |
| USB 修复 | 同八场景连续两轮均通过，退出 0；逐场景 marker 和实际测试数均核对 |
| 默认 / 生产 feature | 普通 provision 六项各通过；生产组合 `product-providers-candidate,email-production-candidate` |
| 其他 Core | 原 saved-trial 7 项、Core 灯控 7 项、Wi-Fi 目录鉴权与私有 D-Bus 2 项通过；不运行热点/recovery 专用套件 |
| Node | network/network-policy/wifi/configuration-state/event-sync/operation-receipt/deadlines/indicator/clients/diagnostics/test-center/test-center-component/console/console-fencing/session/app-imports/app-imports-negative，17 个命令全部 0 |
| 实际浏览器 | Windows Edge，主页装配与控制台各在 320/390/1280 通过；真实 ESM/CSP/IndexedDB/签名和本机有限合成 API；缺模块及拒绝权限负对照通过 |
| fmt | 最终实现快照 `cargo fmt --all -- --check` 为 0 |
| Clippy | network-agent/status-led 全目标全 feature；Core lib/bin/tests 生产 feature 严格 `-D warnings`，均 0 |
| ARM | network-agent/status-led 与 Core 生产 feature 的 aarch64 check 为 0；仅交叉编译，未声称 ARM 执行或实机 |

USB 场景：save→Saved；cancel/renamed/lost-save/revoked/no-usb/no-ip→RolledBack；lost-activation→Unknown。确认、查询和取消都使用原 principal；第二客户端和 Owner 均不能接管该 Client 的 trial。保存过程中撤权触发补偿；未知激活不伪装为已知恢复。no-ip 是人工取消分支，不冒称已验证真实 DHCP 超时。

重跑核心命令：

```text
cargo test -p rustsimhub-network-agent -p rustsimhub-status-led --locked --offline -- --nocapture
cargo test -p rustsimhub-core --lib --locked --offline network_trial::provision::tests:: -- --skip recovery --skip a4_usb_nm --nocapture
cargo test -p rustsimhub-core --lib --features product-providers-candidate,email-production-candidate --locked --offline --no-run --message-format=json
```

最后一条输出中的 **library** 测试二进制才包含新增用例。先在独立 `unshare --net` 中启动私有 dbus-daemon，再以无 capability 的普通测试用户设置 DBUS_SYSTEM_BUS_ADDRESS、RSH_A4_USB_NM=1、RSH_A4_BUS_ROOT（0700 的私有总线目录），运行：

```text
<core-library-test> --ignored --exact network_trial::provision::tests::a4_usb_nm::actual_usb_api_and_nm_adapter_preserve_original_principal_and_owned_rollback --nocapture
```

必须核对 `running 1 test` 与八个 case marker。禁止使用系统 bus 或真实 NM。测试包包含本机实际隔离 runner；路径和 UID 按报告中的本机环境固定，迁移机器需先适配路径，不得删隔离断言。

## 保留的失败与未运行项

- 首次 WSL 启动 Service E_FAIL、二进制副本哈希不一致时均未接受测试；重建落盘校验后复验。曾误选 Core binary 得到 0 tests，已更正为无效结果，不能计通过。
- 沙箱内浏览器启动报 Windows 用户名/密码错误；获授权本机进程重跑才通过。一次回归在磁盘不足时中断，未运行的尾部检查由最终检查补齐，未把中断总报告改成全绿。
- 仅清理本项目可再生 Rust/Go 编译缓存；源码归档、日志及交付包保留。部分归档采用 NTFS 透明压缩并核对前后 SHA256。未调整 WSL VHD、设备或系统网络。
- A4.2 的隐藏 SSID 仍有明确缺口：现有 NM 配置写 `hidden=false`，本轮 Unicode SSID/重命名模拟不能证明隐藏网络发现、定向扫描或连接。需继续补明确契约与隔离测试，不能记验收完成。
- USB 电脑工具外部二进制到实机的端到端、真实 Wi-Fi/DHCP/断电重启/计量准确度和灯颜色电平仍待主线。Core 的审计防重不等于重启后仍能查询旧内存 Session；未新增持久回执 API。
- 热点开发/手机验收及已有 AP 草稿/失败继续保留且暂缓，本轮未新跑热点专用套件。未触碰 BLE/声音/媒体、发布工作流、B 包、设备部署或整体进度。
- GitHub Actions 结果须另行回读，WSL 成功不等于线上 CI 或实机验收通过。

## 集成与下一步

负责人审核两处小范围行为修复及负对照后合并。回退可 revert 本实现提交；无数据迁移，不需要恢复设备配置。新增命令和退出码交 B 作为验证入口材料，未修改其 CI 文件。

下一步继续隐藏 SSID 明确契约、USB 工具端到端接线证据及持久回执边界；现场工作和暂缓热点不标完成。日常一键测试、自动登录/复制、折叠调试、多端回执与实际状态、纯灯优先级本轮复核现有代码，不能重新登记为新实现或提升全局百分比。
