# 成员D：真实Rust GATT跨UID策略集成

2026-10-31（Asia/Shanghai）。仓库Viper-Boss/RustSimHub，分支codex/gtw-gatt-policy-integration-20261030，目标main草稿，依赖#125/#127。起点联合HEAD `329927aab0883dbdd7b6750f3dc3354ee139cd48`；代码/所有正式验证完整HEAD **`70df7c3fab66e4cecb84e22d80e48a584d2f5899`**，tracked_dirty=false；后续仅本人文档，最终HEAD在PR回读。

改动：新增显式手动gatt_kernel_uid target(test=false/required-features)、Rust角色及管道编排；原手动runner新增--rust-gatt和有限失败stderr收集。产品runtime/依赖版本/Cargo.lock/候选policy四材料字节保持，不修改全局台账/他人任务或分支。来源历史作者与AGPL-3.0-only保留，Codex辅助，无工时虚报。

实际命令见[集成矩阵与重现](../ble/gatt-policy-integration.md)。结果：普通BLE **141通过/0失败/0忽略**；严格all-targets/all-features Clippy、fmt/diff通过；新手动编排 **1试验、3个libtest角色各1通过/0失败**；旧头策略手动1试验/9场景通过；材料16通过/0失败/0跳过。没有将角色/场景加入141，不重跑或冒用历史整仓950。

真实不同UID、实际原policy、实际Rust GattExport/BluezControl/PeerLease、标准a{sv}→hq、SCM_RIGHTS/SOCK_SEQPACKET收包、写回、畸形/MTU22/重复FD/foreign拒绝、断连Stale+EOF、注册/撤销/总线关闭均实际执行。BlueZ和host外部端点合成，不把不透明帧当实际密码/业务成功。

| 本地证据 | SHA256 |
|---|---|
| BLE日志 | b3560534c6f0dfeb13ebb94a850ca1a50a7bbf51e6f898916c36e04e91df926b |
| Clippy日志 | a41051563a4cb5fc1b1bcdd9aa0ba28ca80f7736d629d90cc138218756e8e3a9 |
| 新手动container.log | d6b46f4c5c7042c2825eb6e84481a999170130e8140ecd79dd30651a82b5f1b0 |
| 新手动result.json | 5c2fae2216868cfdc18eda13aa724ad0f1c52e7776316f69948345c122b309f4 |
| 受测Rust测试二进制 | 45af7e9c7cec7ed7e9381fc2947969de9b95a71387b31b61dffb61c8871f9de3 |
| 候选policy，与#126/#127相同 | a878a9852c29b51928823e5b8f914feeb1e87d174c609be736b6e18b4bd7900b |
| 旧9场景result.json | 70a33325850f59bf2db9c8727653f18d46394af5e617e57c593e4e4076000d4f |
| 材料16日志 | 4a2b40f50c0160b6d04d332e52b5cb798908d401eecd4fa8e3347ed202213bfb |

逐输入镜像/daemon/宿主公共库/两个Python夹具摘要在本地work/gatt-policy-integration/result.json，不入库二进制。PID/耗时变化会改变重跑日志摘要。没有宿主NSS/system bus/主机BlueZ/HCI/RF、A握手、Core-Wi-Fi、生产Root/安装/设备预算、ARM/浏览器或真实业务验收；不批准候选UID982。默认生产功能未启、schema保持6，没有签名/部署/短信/电话/流量。远端CI按最终HEAD回读，不把账户启动前失败当测试通过。

完整步骤新增0；**21/26离线包已提交、剩5步，固定P0–P7约80%**。独立完成这段组合证据后继续真实BackendV1适配；A会话/迁移和Root接口审核/真实TURN/设备验收仍分别保留。
