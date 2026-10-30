# D-17～20 / D-26前置：现成GATT纯库独立接回main

2026-10-30。当前main缺旧#51的纯库，旧#59清理修复又堆叠在看板分支。
本批只取两条代码提交独立送main，不合入其灯控/看板/任务书依赖、不动旧分支或关闭旧PR。
不重复计D-17～20离线交付，也不是D-22的A密码/实际GATT服务接线完成。

| 来源 | 新分支提交 | 内容 |
|---|---|---|
| `be0219119add5517161a8429328b5c0d1740641d` | `911f91d96f6817aeb6ccdbdb3da6eeecb0cdac1a` | 有界帧、固定manifest、类型端口、Lifecycle/Transport与38项原测试 |
| `99a7f163c23e7afa7cb344bdad4b48b3d66e596d` | `669d8731fce43f3702686d0c40db400292e523e3` | cleanup_failed纳入quiescence、4项补充测试/向量 |

两次cherry-pick -x保留原Author Viper-Boss、AuthorDate和来源，实际Committer GTW。
唯一合并冲突在workspace成员列表：保留当前全部成员（含mdns），只追加crates/ble。
Cargo.lock只多一个本地ble包及已有serde_json开发依赖，没有升级外部版本或增加运行依赖。

## 能做和不能做

库本身仅std生产依赖，forbid unsafe。固定对象/UUID、12字节分片头、2048字节应用信封、
最多256片/15秒重组、4结果队列，严格编号/长度/预算/洪泛处理；分片编号不作为安全nonce。
运行实际Assembler/Outbox、生命周期和Transport，超时保留原future等待迟到结果并补偿，
不自动重发未知注册/通知、不让旧epoch/owner/peer/connection回复复活当前状态。

GattBackendV1/SecurePortV1必须由可信宿主注入；没有默认实现，None secure在注册前拒绝。
Peer/Epoch构造只检查形状，不证明真实BlueZ回调、唯一启动域、密码会话或Core业务授权。
SecureReply变体也不是密码成功断言，不能从网络bool/tag构造。
本库不访问D-Bus/BlueZ/HCI、无线/设备、Auth/业务库，不启动线程/进程/服务或注册广播。
实际标准密码由A、Core桥/可信UID、安装角色和BlueZ宿主仍是前置。

## 清理边界

当SecurePort.disconnected失败、即使本地应用/广播句柄已撤销，cleanup_failed仍使
is_quiescent=false，保持Unknown，禁止宿主过早丢清理责任或自启新轮次。
调用超时并不取消原future；只有真正的迟到成功/适当补偿才可作为完成证据。
拥有Transport/Lifecycle的宿主仍必须保留并poll至明确完成；Drop不是外部清理已完成证明。
不新增重试/延长5秒调用预算，永久失败需宿主监督，不宣称可自动安全恢复。

## 验证范围

在接回的原库修正前runtime上保留同一新增测试，
failed_secure_close_cannot_report_quiescence_after_application_removal实际退出101：
`unconfirmed security cleanup must remain owned`。恢复修复后42项全部通过。
这是原库修正前行为复现，不称其曾在当前main中运行（main原本没有本crate）。

测试直接核对T5冻结JSON/黄金向量，6类预算下1～2048长度、编号耗尽、错帧/重复/跨连接、
固定截止、队列/洪泛、注册/撤销/Release/owner丢失/迟到与清理失败，运行真实生产库，
仅外部端口cfg(test)。不是密码验证、真实D-Bus、BLE硬件或两端浏览器互操作。
完整整仓结果/来源与命令见[交付报告](../deliveries/member-d-gatt-library-main.md)。

没有生产启用、Root/UID/许可/安装改动。库加入workspace只让构建/测试发现它，Core二进制
没有依赖/调用此库，没有蓝牙设备访问；撤回workspace条目/库不需数据库或设备恢复。
旧#51/#59由负责人决定如何收敛/关闭，本批不假称原PR已合并或为其重复计完成。
