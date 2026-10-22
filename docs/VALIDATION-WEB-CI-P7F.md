# P7F：网页安全回归接入自动检查

2026-10-22。用户追加一个小任务；本轮仅修改自动检查配置，补齐已有网页回归脚本的执行入口，没有修改固件业务或操作设备。整体仍 **76.5/100**，不重复计算已有功能的验收点数。

此前GitHub的Rust工作流只执行网页灯控检查，其他四份脚本仅由本地手动运行。现在每次push/pull request都分别执行以下五组，任一失败会使工作流失败：

| 检查 | 命令 | 保护的行为 |
|---|---|---|
| 身份与首次owner | `node tools/test-web-proof.mjs` | 签名原文、设备公钥、用途、角色、权限和代际校验 |
| 音频生命周期 | `node tools/test-web-media.mjs` | 单次激活、迟到麦克风授权取消、静音、失联清理及鉴权拒绝 |
| 一键通话 | `node tools/test-web-call-test.mjs` | 固定目标、仅提交一次、未知结果不重拨、到期挂断、取消和迟到回复 |
| 配置灯控 | `node tools/test-web-indicator.mjs` | 关闭顺序、捕获会话、请求合并及网络失败恢复；保留已有步骤 |
| 网络与月策略 | `node tools/test-web-network.mjs` | 单位/输入、未知无线值、退出后迟到回复隔离、待确认策略不重复提交、隐藏页停止轮询及无自动流量测试 |

保留现有Rust、rootfs、板级和恢复工具检查及只读仓库权限；不新增第三方Action或依赖。运行时版本单独输出，方便定位环境差异。

本地Node v24.19.0下五组全部退出0，`git diff --check`通过。配置提交为`a679da8cb841613ff4d41d68e8fd6ff7a74f7f32`；[GitHub完整运行39723798698](https://github.com/Viper-Boss/RustSimHub/actions/runs/39723798698)已completed/success，五组网页检查、Rust测试、生产配置检查、Clippy及原有系统工具步骤均通过。最终文档提交不会改变这份已验证工作流提交的身份。

证据保存在`.private/web-ci-20261022-first.json`、`web-ci-run-20261022-first.json`和`web-ci-watch-20261022-first.log`。没有访问NAS/棒子、启用普通数据、拨号、发送短信、刷写或初始化正式业务。原卡流量关闭沿用上一轮已验证状态，未在本轮重新测量。

下一项仍为R02首次owner/可信TLS与端点初始化、正式业务启动及冷启动，以及关机返回新boot的诊断；本轮完成后不自动开始。网页模拟检查不能替代真实麦克风、通话、网络传输或硬件冷启动验收。
