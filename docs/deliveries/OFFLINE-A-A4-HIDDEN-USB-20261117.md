# A4 后续：USB 隐藏网络入口与隔离验证

后续：本报告末尾的旧Session重启查询缺口，已在[重启只读回执交付](OFFLINE-A-A4-RESTART-RECEIPT-20261117.md)补离线实现；原受测HEAD保持不变，外部工具及现场仍未验收。

本轮完成隐藏网络的 Core/Wire 代码与本机事务验证，尚未完成整个 A4 的工具/现场验收。热点继续暂缓，整体固定进度 **81.5%** 不变。

## 固定来源

- 起点/上一轮报告 HEAD：`67c17924f1ffefae7ea55e7dd4e2b0a05cfaff12`；main 回读仍为 `de663743bd726e77f7c560139d0f91ea78e8fe57`，PR274 尚未合并、暂无新评论。
- 本轮 `implementation_head`：`72bd90fa9b1adb50ed7eac560b582602c83eca09`。
- 实现树：`a55b08874b197853b985ec00afb0fbf4bf0f6b16`；源码归档 SHA256：`2b9de717baeb36cc19fee870dadffcf472fdf6272ce0612da2c863f973608abb`。
- 当前完整报告提交 HEAD 在 PR 正文记录，报告自身不循环引用。

## 已实现

1. Wire 的独立 `wifi_begin_hidden`，沿用原严格密码/SSID/幂等键约束，不改变 `wifi_begin` 字节形状；未知字段、任意网络参数均拒绝。
2. 原 authenticated IPC 路由接入同一个 provision actor；health/capabilities 增加 `wifi_hidden_provisioning_api`，值受原启用状态约束。无新角色、无限权限、Root 任意操作或旁路。
3. 隐藏意图写 NM hidden=true，试连/确认保留同一模式；模式加入原 principal 绑定的幂等指纹，切换模式不能套用旧回执。
4. NM 精确 profile 复核 hidden；false 缺省兼容，隐藏标志丢失失败关闭。旧 volatile / 确认 / checkpoint / 撤权补偿 / Unknown 继续使用原实现。
5. 扩展私有 D-Bus 测试至 17 场景，并新增真实 IPC 关闭状态拒绝、Wire 严格解析测试。

修改仅 `crates/wire/src/auth.rs`、`crates/core/src/authenticated.rs`、其 `tests.rs`、`network_trial/provision.rs` 和 `provision/tests/a4_usb_nm.rs`，加本包文档。未改 protected recovery/BLE/AirSIM/音频/发行/工作流/全局进度/设备；共享 NmProvision 增加默认 false 模式，受保护调用者仍取 Default，无签名变化。来源为自有实现，沿用 AGPL-3.0-only，AI 辅助开发与测试。

## 实测证据

环境：固定 Rust 1.98.0、Ubuntu-24.04 WSL2、Cargo locked/offline；网络命名空间只有 lo，测试用户 UID1000、CapEff/CapBnd=0、NoNewPrivs=1，私有 dbus-daemon 最终停止。原主机 Wi-Fi/路由/DNS/防火墙无修改。

| 命令/验证 | 结果 |
|---|---|
| Core lib/bin，生产 `product-providers-candidate,email-production-candidate`，`cargo test --no-run --message-format=json` | 均 0，副本落盘 SHA256 核验 |
| Core lib 默认 `cargo test --lib --locked --offline --no-run --message-format=json` | 0；复制实际库测试二进制 |
| 默认与生产库二进制 `--ignored --exact network_trial::provision::tests::a4_usb_nm::actual_usb_api_and_nm_adapter_preserve_original_principal_and_owned_rollback --nocapture` | 各实际运行 1 个顶层测试、17 场景 marker 齐全，均 0；不算 34 个独立功能 |
| `cargo test -p rustsimhub-wire --lib --locked --offline hidden_wifi_is_explicit_bounded_and_does_not_change_legacy_commands -- --nocapture` | 实际 1 项，0；新旧 op、序列化及 22 个字段拒绝变体 |
| 同 crate `wifi_provisioning_bounds_credentials_and_rejects_network_overrides` | 实际 1 项，0 |
| Core bin exact `authenticated::tests::hidden_usb_wifi_real_ipc_advertises_disabled_and_never_bypasses_the_mode_gate` | 实际 UnixStream frame/解析/分发 1 项，0；当前 Owner 和未知令牌均不能绕过关闭模式，能力 false |
| 生产库 `network_trial::provision::tests:: --skip recovery --skip a4_usb_nm` | 原 6 项通过 |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy -p rustsimhub-core -p rustsimhub-wire --all-targets --features rustsimhub-core/product-providers-candidate,rustsimhub-core/email-production-candidate --locked --offline -- -D warnings` | 0 |
| 同两 crate `cargo check --lib --bins --features <同上> --target aarch64-unknown-linux-gnu --locked --offline` | 0，仅 ARM 编译 |
| Node network/network-policy/wifi/configuration-state/operation-receipt/event-sync/clients/session/app-imports/app-imports-negative | 10 个命令均 0 |
| 实际 Edge，`tools/test-web-app-assembly.mjs`、`tools/test-web-console-browser.mjs` | 320/390/1280 均通过；实际主页/ESM/CSP/IndexedDB、原ID恢复、拒绝权限、迟到结果、缺模块负对照；API为有限本机合成服务 |

17 场景＝原普通 8 项＋隐藏 8 项（save/cancel/renamed/lost-save/revoked/no-usb/no-ip/lost-activation）＋隐藏 profile 标志丢失。错网络与撤权不能保存；激活回包不明、隐藏标志丢失保持 Unknown。no-ip 是无 IP 后取消，不冒称真实 DHCP 超时。NM 对普通 false 的省略也在同一 GetSettings 适配中验证。

详细工具契约见 [接口说明](../offline-a/a4-usb-hidden-interface-20261117.md)。本轮没有改受保护工具源码，因此不宣称“电脑工具已完成隐藏网络功能”。没有运行新热点专用测试；构建共享依赖不等于执行其测试。

## A4 剩余状态与边界

| 条目 | 当前状态 |
|---|---|
| A4.1 USB共享试连/查询/确认/取消、原principal | Core离线事务/拒绝验证已交，工具/实机端到端未验收 |
| A4.2 隐藏SSID/重命名/断开/回退 | 本轮补显式Core入口和NM模拟；真实RF/DHCP/USB保持未验收 |
| A4.3 月结/时钟/多接口/缺值/重启 | 上轮固定计量实现及63网络/纯灯测试证据保持；本轮没有改计量字节，不重复计完成 |
| A4.4 一键/登录/复制/折叠调试 | 复用现有日常路径，真实主页装配/10专项再次通过；外部工具尚未改 |
| A4.5 多端回执与实际状态 | 原principal/模式/原ID/保存和当前profile差异已验证；Core重启旧Session查询仍有限制 |
| A4.6 三宽真实模块/CSP/IDB | 本轮再次通过；没有把合成API说成真实设备 |
| A4.7 纯灯模型 | 上轮证据保持，本轮未改；实机颜色/电平归主线 |
| A4.8 文件/报告/审核 | 本轮精确实现和证据提交原PR274；未自审核、合并或部署 |

下一步独立工作是明确旧 Session 重启回执契约及其拒绝/防重验证。现有审计防重不能宣称持久查询已实现；如果要新增查旧回执/迁移格式，应先给出准确协议和恢复来源，而非重发旧试连。外部工具、实机及暂缓热点均保持真实未验收。

回退：revert 本实现提交；无数据库迁移，不删除旧审计或改设备配置。新增 op 在旧服务中会拒绝，工具必须按能力字段处理，不回退发送普通意图。B可从 implementation_head 取固定源码及命令自行验证；未修改 B 的入口。CI 在当前最终 HEAD 另行回读，不能用 WSL 通过代替线上绿灯。
