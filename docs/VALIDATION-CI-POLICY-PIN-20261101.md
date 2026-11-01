# 主线 CI 策略摘要修正

GitHub run `41275492149`（#148）在 Rootfs tooling fixtures 的 36 项测试中有一项失败：`rust-core-policy-not-exact`。尚未运行到完整 Rust workspace，不把相关包本地通过称作整套 CI 通过。

原因：main 的 `7a6da18` 给 `rustsimhub-forwarder` 增加硬件显式拒绝，但 rootfs 审计仍固定到旧策略的 SHA-256。本次审阅现有规则：仍只给 Core 有限 Messaging/Voice/network-control/checkpoint-rollback/settings.modify.system 权限，其余项目用户被拒绝。策略文件本身不修改；审计固定摘要更新为其真实 Git 字节 `1712437963bf11875e63ffce7aa74c669841e53bd25d4422f4adec87d6875f04`，不改为动态信任、不跳过审计。

反例测试增加“删改 forwarder 身份仍拒绝”。CI 同时补跑已有的首次设置真实页面测试，避免只测控制器遗漏按钮接线。

设备未连接、未安装新策略，验收81.5%不变。完整CI结果以本修正的最终运行记录为准。
