# 首次配网（开放热点）交接给 Codex — 2026-10-29

用户 2026-10-29 决定：首次配网这一项改由 Codex 修复，Claude 停止该项开发与实测。本文只记录事实，不代表已验收。

## 1. 代码在哪里

- 已合并到 `main` 的部分（基础功能）：
  - `69953d9` 无设置码的首个 Owner 认领、密码派生密钥（`crates/auth/src/setup.rs`、`crates/edge/src/web/password.js`）
  - `8d183fd` 首次配网专用开放热点 RustSimHub-XXXX（`crates/setup-ap/`）
  - `62d0bb9` 热点开启前的 Wi-Fi 列表快照（`crates/core/src/network/wifi.rs`）
  - `ca22f85` / `746559f` captive 明文 HTTP 配置页（`crates/edge/src/linux/captive.rs`、`crates/edge/src/web/setup.{html,js}`）
  - `426cc2e` 页面解包 Core 应答信封；`e78e9be` 未有 Owner 时一直保持热点；`0043515` 一小时检查点、限频重开
- 未合并，在分支 `claude/ap-reconfigure-impl`，PR https://github.com/Viper-Boss/RustSimHub/pull/62（OPEN，6 个提交，HEAD `9dae748`）：
  - `fa0c1d9` 忙锁不取消轮次、保活限频（≤每 5 s，避免 1024 序号耗尽）、列表刷新按钮
  - `7f2a2c6` 日常 Edge 不再跟随 192.168.100.1
  - `ce6c868` Root 可用性在窗口内过期不再阻断；登录模式隐藏确认密码、手机排版
  - `bac11e3` Root 探测偶发失败容忍 3 次；captive 页独立限流
  - `90afa2f` 开放热点在设备未配好前永不因探测失败关闭；首配会话不要求 token 新鲜度
  - `9dae748` 已有 Owner 但没保存 Wi-Fi 时继续提供首次配网（`LocalSetup::for_owned_device`、`saved_stations()`，读失败按“已配置”处理）
- 设计/测试记录：`docs/DESIGN-AP-FIRST-SETUP-OPEN-2026-10-28.md`、`docs/DESIGN-AP-FIRST-SETUP-OPEN-2026-10-29-PERSIST.md`、`docs/PLAN-FIRST-SETUP-FIELD-TEST-2026-10-28.md`（实测第 1–5 轮）、`docs/VALIDATION-APP60-DEPLOY-2026-10-29.md`。

## 2. 本地测试（9dae748）

- 认证 44、Core 170、setup-ap 100、launcher 30、wire 19 通过；clippy `-D warnings` 无告警。
- 全仓回归（WSL）：691 通过，7 失败，均为已知环境失败（forwarder egress ×5、compat go_nas 无 Go、compat preflight_cli PoisonError 连锁）。
- Web：`tools/test-web-setup.mjs` 等全部通过（mock 返回真实信封）。

## 3. 设备当前状态（2026-10-29 02:02Z）

- 已确认应用 epoch 66 = `9dae748`（manifest `cba97593…`），floor 66，恢复额度未用；7 个服务 active，wlan0 连回家庭 Wi-Fi，普通蜂窝数据关闭。
- Root setup-ap helper 已换成 9dae748 版本；旧 helper 与 setup-edge/setup-ap 安装备份在设备 `/var/lib/rustsimhub/rustsimhub-v1/system/app66-backup-20261028-first`。
- 临时测试身份树仍在 `/var/lib/rustsimhub/rustsimhub-v1/first-setup-test-20261028`（含 `restore.sh`；各轮旧数据在 `before-*` 子目录）。已执行恢复：/run drop-in 已删、45 分钟恢复定时器已停、真实数据在用。
- 应用 65 的半上传目录已删除。

## 4. 实测已确认 / 未确认

- 已确认（前几轮）：wcn36xx 上开放热点可开、手机拿到 DHCP；安卓（MIUI）弹出 captive 页；可创建 Owner。
- 用户在 app 66 测试后反馈“还是有问题”，具体现象未记录 —— Codex 接手时请先向用户确认现象。
- 从未在真机通过：Wi-Fi 试连与保存回执；“刷新列表”后断连是否消失；列表快照；断开重连后切登录模式；有 Owner 无 Wi-Fi 重启后重开热点；iOS 弹窗。

## 5. 已知坑（实测踩过）

- systemd drop-in 里空 `BindPaths=` 会同时清掉 `BindReadOnlyPaths=`（一张表），必须重新加回程序绑定。
- NM checkpoint 超时上限需 ≥ 窗口长度 + 30 s，否则一小时轮次被拒、出现重开自旋。
- Root 的可用性在窗口内不刷新；Core 在轮次开始时记录支持状态。
- 设备重启后 NAS 侧 USB 网络需 `nmcli connection up` 对应 profile，USB 隧道才能用；AP 开启时 Wi-Fi 管理不可用，只能走 USB。
- 部署脚本 `.local/*-appNN-*` 由上一版文本替换生成：注意 `manifest.sig` 长度是 64（app 65 上传失败就是被误替换成 65）。

## 6. 部署/测试脚本（本机，不提交）

- `.local/{freeze,build,build-audio,regression,sign,verify,preflight,upload,prepare,backup,deploy,verify-attempt,health-confirm}-app66-*`
- `.local/first-setup-test-{prepare,start,restore}-20261029-first.py`；证据 JSON 在 `.private/`。
- 所有设备写入、部署、重启仍需用户逐次批准；不写基带/NV/校准分区。
