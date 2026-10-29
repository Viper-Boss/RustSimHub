# USB 工具改 Wi-Fi 的设备端授权（2026-10-29，代码与隔离测试）

背景：用户决定 Wi-Fi 重连由独立的 Windows USB 工具（RustSimHub-UsbTool，GPLv3）经 USB 完成。工具持有自己的 Ed25519 密钥，经主人批准配对（`kind=client`，`status.read` + `network.manage`），不走网页配网。本记录只覆盖设备端代码；**未部署、未连真实设备**。

## 实际完成

1. **Core 权限**（`crates/auth/src/lib.rs`、`crates/core/src/network_trial/provision.rs`、`crates/core/src/network/wifi.rs`）
   - 新增 `Auth::network_manager(token)`：复用已有的 `authorize(token, network.manage)`（会话对应的存储 principal 必须仍 active、key/kind/scopes/generation 与会话一致），再按类型：
     - `owner`：仍须 `recent_owner`（最近 5 分钟交互登录），与原行为一致；
     - `client`：已被主人批准且带 `network.manage` 即可；
     - `nas`：一律拒绝，即使被批准了 `network.manage`。
   - `wifi_networks`、`wifi_begin/status/confirm/cancel` 及试连接过程中的复核（原 `OwnerGate`，现 `ManagerGate`）改用它。其他主人专属操作（`open_pairing`、`pending_pairings`、`approve`、`revoke`、`clients`、配网热点 setup_ap_* 等）不变，仍走 `owner`。
   - 试连接仍绑定开始它的**完整 principal**：查询/确认/取消要求同一 trial_id 且同一 principal；同一幂等键只在同一 principal + 同一输入指纹时返回原试连接（抽成 `Session::owned_by` / `Session::replays`，逻辑不变）。
2. **网页批准入口**（新 `crates/edge/src/web/clients.js`，`index.html` 的“连接与配对设置”里新增“配对审批（所有者）”）
   - “允许新设备配对（5 分钟）”按钮（确认后调用 `open_pairing`，并提示会作废未批准的申请）；
   - 待批准列表：申请编号、类型、公钥全文（64 位十六进制）、权限名称（如 `network.manage（管理网络（含改 Wi-Fi））`）、剩余秒数；
   - “核对后批准”：确认框再次显示编号/公钥/权限，`approve` 原样带回同一 `pairing_id`、`public_key`、`kind`、`scopes`；回复的公钥/类型/权限与申请不一致时不报成功；
   - 已授权客户端列表（不含所有者自身）和“撤销”按钮；
   - 批准/撤销结果不明时不自动重发，提示刷新；刷新后按 Core 列表核对上次操作是否生效；退出登录后迟到的回复不渲染，旧列表的按钮失效；一次只进行一个操作。
   - Edge 新增 `/clients.js` 路由（与其他脚本相同的 Host/缓存边界）。
3. **USB 上可访问日常 Edge**（`crates/edge/src/linux/addresses.rs`、`crates/edge/src/linux.rs`）
   - 核对：Edge 的 Host/Origin 检查只按请求头选择已接受的网址对，不看 TCP 本地地址；日常 Edge 安装（`--prepare-local-media`，名称 `rustsimhub-XXXX.local,<Wi-Fi IP>`）的主网址和证书主名称就是 `.local`（新增测试断言证书 DER 含该名称）。
   - 设备上的日常 Edge 安装配置没有 10.55.0.2（APP58 记录：监听地址仍为原 Wi-Fi 地址，`.local` 为主网址）。为避免重新生成安装材料/证书（会触发“explicit migration required”并要求浏览器重新信任），改为代码跟随：**主网址为 `.local` 时**，除配置地址和 wlan0 当前地址外，还监听固定的 usb0 地址 `10.55.0.2`（与出厂 `usb.nmconnection` 一致）。地址未分配时跳过并每 5 秒重试，与 wlan0 跟随相同；配网 Edge（主网址 192.168.100.1）不受影响；监听总数上限 8 不变。

## 验证命令及结果（WSL Ubuntu，`CARGO_TARGET_DIR=~/rsh-target`）

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`：通过。
- `cargo test -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-edge -p rustsimhub-wire -p rustsimhub-policy`：core 166、auth 44、edge 37、wire 19、policy 1，全部通过。
- 新增/修改的失败路径测试：
  - `auth::network_manager_is_recent_owner_or_live_approved_client_only`：已提交证明但未批准（无法登录、未知令牌拒绝）、批准后可用、工具拿不到 `recent_owner`/`open_pairing`/`clients`/`pending_pairings`/`revoke`、撤销后立即拒绝、重新批准后 generation 增加且旧令牌仍拒绝、存储侧撤销拒绝、缺 `network.manage` 拒绝、`kind=nas` 带 `network.manage` 仍拒绝、主人登录超过 5 分钟返回 Expired。
  - `core::network::wifi::catalogue_needs_owner_or_approved_network_client_and_rechecks_after_read`（替换原“只允许主人”测试）：缺权限、NAS、未知令牌在读取前被拒；批准的工具可读；读取期间被撤销则丢弃结果，之后拒绝；Core 身份重启仍使会话失效。
  - `core::network_trial::provision::approved_client_trials_stay_bound_to_their_principal_and_end_on_revoke`：工具只能查询/操作自己的 trial_id，另一个客户端和主人都不能；幂等键换输入或换 principal 都拒绝；试连接中的复核对错误令牌拒绝，主人撤销后立即拒绝。
  - `edge::addresses::usb0_is_followed_only_under_the_local_name_and_never_duplicated`。
  - 原配网热点测试（`setup_ap_control::station`/`bootstrap` 中“已配对的 network 客户端不能发布/确认”）仍通过，热点路径仍只认主人。
- 网页：`tools/test-web-*.mjs` 共 19 个全部通过，其中新增 `tools/test-web-clients.mjs`（真实模块 + 合成 DOM：精确回传批准、拒绝确认不发送、窗口被拒、格式异常不渲染、批准/撤销结果不明不重发且刷新后核对、回复不一致不报成功、退出后迟到回复和旧按钮无效、并发只允许一个）；6 个加载 app.js 的既有测试补了 `ClientsPanel` 桩。CI 增加该测试步骤。

## 未验证 / 需要用户

- **未部署到设备**，需要用户另行批准。部署后需要实测：电脑经 USB 设 10.55.0.1/29 → 工具连 `10.55.0.2:8443` 钉证书 → 主人在网页“允许新设备配对” → 工具申请 → 网页核对公钥批准 → 工具读 Wi-Fi 列表、试连接、确认/取消 → 撤销后工具被拒。
- 设备上 Core 是否开启了 Wi-Fi 试连接（`provision::enabled()`，未开启时所有 wifi_* 一律拒绝）需部署时核对。
- 换 Wi-Fi 期间 USB 连接是否保持（理论上不受影响）未实测。
- 浏览器端批准页面未在真实浏览器中打开过（只有合成 DOM 测试）。

## 工具端对接要点（协议差异）

- Core 拒绝一律回复 HTTP 200，`result` 为 `{"ok":false,"error":"denied"}`，不区分未批准/权限不足/主人登录过期/忙（通话中或已有试连接）/输入错误。工具不能据错误文字判断原因。
- 待批准列表和已授权列表里 `public_key` 是 32 个整数的数组，`scopes` 是位掩码整数（`status.read`=1，`network.manage`=64，合计 65）；`approve` 请求里 `scopes` 是权限名数组。
- Edge 对请求格式错误（如令牌不是 64 位）返回 HTTP 400，Host/Origin 不符返回 403，Core 不可达返回 503；结果不明只应出现在 5xx 和连接中断。
- 主人重新点“允许新设备配对”会作废尚未批准的申请；申请和窗口 5 分钟到期。撤销后工具需重新申请并由主人批准；重新批准后 generation 变化，旧令牌失效，必须重新登录。
- 同一时间只有一个试连接；有未结束的试连接（含网页发起的）或通话占用网络时，新的 `wifi_begin` 被拒（同样是 `denied`）。
