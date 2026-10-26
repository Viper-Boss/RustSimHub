# P4Q：首次认领后的 Owner 配网接管

2026-10-26。AP-05 的源码路径已接通：通过真实 LocalSetup 完成签名认领的第一个 Owner，可以沿用正在运行的配置热点进入 P4P 试连与保存页面。原热点绑定、执行序列、独占网络锁和截止时间全部保留。

固定源码 `11f614dddb1052c8fed50cd2f02aabc9db1381d8`；662 个文件逐字节核对 Git，归档 SHA-256 `4323651169ef75c28f79f9fd2ed6cab730effd0fe40e3c2ec09c132c1311d76a`。本轮没有部署、访问设备、切网、拨号、发送短信或推送；普通蜂窝数据与 PMIC 约束保持。

## 页面与接口

首次认领成功并登录后，用户仍只需填写 Wi-Fi 并点“开始试连”。页面识别 `first_setup` 和显式 `owner_adoption_required`，先持久记录独立随机键，再调用现有 `setup_ap_start`。Core 将原热点交给这个 Owner，返回原 operation_id，随后使用既有试连/确认接口。没有增加匿名配网 API。

接管回复丢失时，浏览器仅按已保存的键摘要查询原记录。部分交接状态仍由原 bootstrap 处理取消时，页面核对精确操作 ID，再查询实际终态；取消应答本身不算完成证明。原 IndexedDB 单事务、凭据仅在内存、关闭清理和不自动重提继续适用。

## 授权与会话边界

- 要求最近登录的真实 Owner、network.manage、真实电话空闲，以及原 LocalSetup 的当前签名认领者精确匹配。
- 复查窗口 ID、realm、设备公钥、Core 实例、Owner 公钥/代次/权限；普通 Auth bootstrap、委托网络客户端、错误 token、被替换的窗口均不能接管。
- 要求原热点 Ready 且已确认，Root Ready 证明新鲜、绑定和序列一致；当前材料及试连/保存能力必须匹配。取消、旧回执、忙通话或其他网络操作占用时拒绝。
- 转交期间同时持有两份状态锁，移动原独占 guard；交接不发布新的 Start、不刷新原创建时间或截止时间。接管后按现有 Owner 规则维持心跳。

原认领窗口不足、已到期或失效时仍拒绝接管。过期后如何在无既有网络时重新进入维护配置，继续由失败恢复和独立操作手册覆盖；本轮没有放宽首次认领或重置已存在的所有者。

## 持久交接与中断

私有 journal 先写不可变 `first-setup.owner.json`，记录精确原 bootstrap 与 Owner 元数据；随后写 Owner intent、Owner pending，再删除原 bootstrap pending 并同步目录。交接文件不含密码、SSID、登录 token 或设置码。

Core 重启时先验证交接证明及原绑定，补齐精确元数据，再进入既有取消/Unknown 恢复。它不会从磁盘恢复登录、重新启动 AP 或重做试连/保存。冲突的 pending、变更绑定或非规范摘要拒绝，读取路径始终固定且受限。

如果中断发生在原进程内，原 bootstrap 仍持有 guard 并负责清理。收到精确 Root Closed 后先持久保存对应 Owner 结束证明，再清理两个 pending，最后释放锁。之后查询或重启仍能读到这笔结束回执。完整交接后的保存则沿用 P4O 独立 Saved 证明。

旧 bootstrap tick 在等待电话检查时与接管并发，恢复后会发现原状态已移交；不会取消新的 Owner 会话。该时序已有可控通知同步的实际并发测试。

## 已验证

最终 **510 项 Rust、0 失败**，严格全工作区 Clippy、生产检查及 Core/Edge/setup-ap 主机构建通过。检查在仅回环命名空间、普通 UID、空组/能力下执行。

新增 12 项真实 LocalSetup/Auth、私有文件和独占锁测试：

| 范围 | 验证动作 |
|---|---|
| 正常接管 | 原绑定/序列/截止不变、重复键查询、不同键忙、持续持锁、关闭后可重启 |
| Root 前置 | 错绑定、Closing、材料变化、已取消均不创建交接 |
| 认领边界 | 通用 Auth bootstrap 无法替代真实设备认领 |
| 权限 | 错 token、忙通话、替换的 LocalSetup 拒绝 |
| 委托 | 已签名且有 network.manage 的客户端仍拒绝 |
| 完整 Core 链路 | 新状态认领→原 AP 接管→试连→确认→Saved→释放，journal 无凭据 |
| 三个中断点 | 仅交接证明、已有 intent、已有 pending；重启只取消原绑定 |
| 写证明失败 | 保留原 bootstrap 清理与 guard |
| 写 pending 失败 | 从已落盘交接恢复元数据，不重发 Start |
| 冲突证明 | 路径摘要、原绑定、Owner pending 不一致时拒绝 |
| 原进程收尾 | 中断后取消、精确 Closed、持久查询、再重启 |
| 并发 | 旧 tick 在电话检查期间被接管，不误撤销 Owner |

上述 Root 网络结果明确使用回执夹具，尚待跨进程真实 NM 与硬件验证。首次测试编译因 Hint/Binding 有意不提供 Debug，断言写法不兼容而失败；改用不输出私有对象的布尔相等断言，失败记录保留。后续完整链路及最终第四轮检查通过。

15 组网页回归通过，新增初次认领接管、丢失接管回复后只读恢复及部分交接取消。页面布局沿用 P4P；本轮修改状态接线，没有重新宣称设备网页验收。

固定归档以全新空 target 离线构建三份生产 ARM64 release；AArch64/GLIBC 边界、Core/Edge QEMU 同源摘要、开发夹具关闭、实际程序内交接与网页标记，以及 AP 无效命令拒绝均通过。

精确源码 [CI40266469773](https://github.com/Viper-Boss/RustSimHub/actions/runs/40266469773)两个任务均 success，含510项Rust、15组网页、实际材料CLI、原生音频和真实转发服务协调。前一文档提交 ab52630 的 CI40249820572 已两个任务 success，与此前保留的失败记录分开。

## 剩余范围与依赖修正

AP-05 按合成新状态的首次所有者流程及已认领边界计完成。AP-03 保留“真实保存后重启仍可连接”的验收；AP-06 负责同源联合安装材料，AP-07/08 再验证热点隔离、目标网络与证书/身份/地址交接，AP-09/11 验证无 NAS 供电及冷启动。

看板修正 AP-06 的前置为 AP-01 与 AP-05：材料需要已冻结的授权/保存契约和认领接管源码；材料先准备并安装，才能验收 AP-03 的真实重启连接。原列表让 AP-06 先等待 AP-03 整项验收，形成了实际交付顺序上的循环。本次只修正前置顺序，169 个动作、所有原验收、估时和权重均保持。

原 P0–P7 仍 **80%**；剩余台账 **9/169 完成、余160、加权3.6%**，剩余估时190.5–453个有效工程小时，外部等待另计。实机分值保留到对应验收。当前不需要用户操作，下一步准备联合材料与真实 Core→Root→NM 联调。

源码：`crates/core/src/setup_ap_control/bootstrap/adoption.rs`、`bootstrap/adoption_tests.rs`、`bootstrap.rs`、`setup_ap_control.rs`、`crates/edge/src/web/ap-wifi.js`、`tools/test-web-ap-wifi.mjs`。
