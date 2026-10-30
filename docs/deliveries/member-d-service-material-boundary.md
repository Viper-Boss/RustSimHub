# 成员 D：D24 服务材料边界修正

仓库 Viper-Boss/RustSimHub，分支 `codex/gtw-ble-material-boundary-20261030`，目标 main。main 基线 `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。

原作者 Viper-Boss 的 `00f87779ff79ca680433e0a0b5ed35c7d19437c1` 以 cherry-pick -x 保留 Author/日期，得到 `2603af1a5218444ee55f88fea5bb9a174793d7a7`；本人不修改旧 #53 或其他分支。修正/受测代码 HEAD **`da818e71fcbb7421e9b7d5917fb77281e7b442f0`**，最终只追加本人提案/报告文档，最终交付 HEAD 见 PR 回读。

修正前实际探针：旧检查器接受 `SupplementaryGroups=rustsimhub-ipc`，退出码 1/AssertionError；修正后正式拒绝断言通过。提案 v2 无附加组、使用新专用恢复目录候选；通知候选仅固定 AcquireNotify 路径/至多 1 个输出 FD，不允许全局 Value signal。检查器仍只读、永不批准安装；不包含 daemon、账户/目录创建或新生产 Root 格式。

精确受测命令：

```sh
python3 -B firmware/system/ble/test_inspect.py -v
python3 -B firmware/system/ble/material_plan.py
git diff --check
```

16 通过、0 失败、0 跳过；退出码均 0。实际 systemd-analyze verify、实际私有 dbus-daemon/dbus-send 边界和现有 setup_ap_install.identities 合成输入均运行，不只比较手写 XML。测试日志 SHA-256 `997d3ac97aca0c0e6d907dac4c262eacfe2b6c68a34e2dbde4d94d9aecfcea87`；输出计划 SHA-256 `46c41ca7cf0ec11ce0725052f9b8f7f3f0aa47ab91778dbf3d124498dbaac85a`。日志含耗时，重跑摘要可变；模板字节/hash 在计划中逐项输出。日志仅本地 work，未将私有/无关文件加入提交。

没有 Rust 改动，不声称本分支运行了其他栈 950 项回归。无生产 NSS/双 UID BlueZ/SCM_RIGHTS/真实 HCI/RF/资源/ARM/设备/业务或 A 握手；没有修改签名、部署、主线全局台账/贡献归属，未联网到设备/提供商、未拨号/短信/蜂窝。真实系统叠加策略、命名空间、角色/UID 和 Root 安装仍待主线审核。候选 982 未预留。完整解释及下一步见 [材料提案](../ble/service-material-boundary-proposal.md)。

原 AGPL-3.0-only/作者与来源保留，Codex 辅助；无实耗虚报。新增完整步骤 0，固定 **21/26 离线包已提交、剩 5；P0–P7 约 80%**，不将设计/离线提交计为完整验收。远端 CI 依据最终 HEAD 回读，不修改账户限制、不把未启动测试写作通过。
