# P4M：所有者授权的热点内 Wi-Fi 试连通道

2026-10-26。AP-01 的 Core 授权与 Root 请求契约已接线，固定源码为 `3516f4569daba260982cb1fdf44f664d05efb7e6`。这是源码与隔离回归验收；尚未交付设备、开启热点或切换实际 Wi-Fi，原 P0–P7 仍为 **80%**。

## 有限接口与权限

Wire 增加三个有限命令，沿用已认证 Core 分派路径：

| 命令 | 输入 | 授权与用途 |
|---|---|---|
| `setup_ap_wifi_begin` | token、operation_id、trial_id、SSID、passphrase | 当前 Owner、network.manage、最近五分钟登录；提交前检查通话空闲，再复核主体、realm、设备和 Core 实例 |
| `setup_ap_wifi_status` | token、operation_id、trial_id | 当前 Owner 与 network.manage；只返回绑定该所有者的进度和回执 |
| `setup_ap_wifi_cancel` | token、operation_id、trial_id | 最近登录的当前 Owner；只取消完全匹配的试连 |

两个 ID 均为规范 UUID。Root 请求中的所有者公钥摘要、generation、完整父 AP Binding、时间和期限由 Core 派生；前端不能提交身份、任意设备路径、设置字典、shell 或蜂窝操作。Root 元数据仍是可信本地 Core 的声明，不能独立证明远程 Owner。

仅显式 `--serve-configurable` 的 Root worker 声明 `station_trial_supported=true`。旧 availability 缺失该字段时为 false，legacy false 序列化继续省略。能力在父 AP 开始时捕获；活跃 AP 不依赖“允许新开 AP”的 availability，试连必须另有新鲜、精确匹配的 Root Ready 回执。旧 USB Wi-Fi 流程和既有父 AP 持久记录格式保持。

## 生命周期与失败语义

1. 独占原 Core 网络事务锁，要求父 AP 有效、未取消、Root Ready，父会话至少余 60 秒。每笔试连最多 40 秒，每个父会话最多 16 笔，预留父会话清理时间。
2. 先持久写入不含 SSID、口令或 token 的不可变 intent 与 pending，再发布固定受保护槽。槽仍为 Core:Root 的 640 文件，目录 2750，大小上限 8192，拒绝链接与错误权限。序列化、替换旧文件和读取秘密时的缓冲使用 Zeroizing。
3. 同 ID、同载荷只查询已有记录，绝不重新发布 Test；同 ID 换载荷拒绝。已消费但结果未知的 ID 不能当作新请求重试。
4. Root 的 Testing 确认后清除 Test 口令；旧 Testing 回执不能删除稍后提交的 Cancel。取消已结束的旧试连也不能覆盖另一笔新试连。
5. 只接受原请求摘要、父会话、主体代次、新鲜时间和 HTTPS 返回证明完全匹配的回执。早于请求或未来的采样、过期采样、错代次和缺失 HTTPS 证明不能形成成功。
6. 15 秒未获确认或达到试连截止后保留 Unknown，发精确 Cancel 并收尾父 AP；不会重新试连。Core 恢复仅发取消，不恢复 Test。父网络锁要等 Root Closed 证明清理后才释放。
7. 父 AP Closed 可以清除该事务的秘密与 pending，但不能把未确定的试连改写为成功。旧 Core 回退后留下的试连文件，仅在已有精确父 Closed 私有证明时做同范围清理。

状态只返回 ID、状态、是否历史回执、AP 返回证明及是否需要后续确认；不回显凭据，也不声称已经持久保存 Wi-Fi。父 AP 到期或跨 boot 后不再显示可确认的活动试连。

## 验证与边界

最终本地 `cargo +1.98.0 test --workspace --all-features --locked --offline` **459 项通过、0 失败**；严格 workspace/all-targets Clippy、生产 Core/forwarder 检查及生产 Core/setup-ap 主机构建通过。执行处于只有回环网卡的网络命名空间，降为普通用户、清空组与能力，未访问设备。

新增 12 项 Core 试连测试使用真实签名 Owner、真实 Auth、实际私有 journal 和受保护文件槽；Root 回执明确由夹具模拟。覆盖有效请求、无能力或父 AP 未就绪、错误主体、错误代次/HTTPS/采样、活动通话拒绝、重复载荷、旧取消与新事务隔离、口令清理、无确认超时、发布失败、Core 恢复及父会话清理。协议/文件测试覆盖 capability 兼容、固定槽清理和 Wire 输入限制。既有 USB 网络事务测试随全工作区回归通过。

第一次故障注入测试在读取夹具 Hint 时提前遇到模拟权限错误，并未测到目标发布路径。已改为在权限变化前捕获父 ID，第二轮通过；随后增加采样下界、活动期限与两项事务隔离检查，第三轮 459 项和严格检查全部通过。三轮原日志均保留在私有目录，不覆盖失败记录。

固定源码 643 文件归档逐字节与 Git 核对，SHA-256 为 `aed623a485907407beaff87af96623fcf9bfc01f820f3492439dc6fa05d78589`。归档源码的生产 Core/setup-ap 两程序 ARM64 release 构建通过，ELF 均为 AArch64、GLIBC 需求不超过设备边界。QEMU 实际 Core 元数据确认相同归档摘要、schema6、开发夹具关闭；setup-ap 无效命令有界拒绝。未调用无线服务入口。精确源码的 [CI40241225479](https://github.com/Viper-Boss/RustSimHub/actions/runs/40241225479)两个任务均 success，已在后续轮次核对；不据此推定后续保存事务也已通过 CI。

尚未验收：真实 Core socket 到 Root 无线操作联调、AP 配置网页、最终持久保存、首次 Owner 认领后接管 bootstrap AP、联合签名安装、硬件错误密码/回退/冷启动。AP-02、AP-03、AP-05、AP-06 及后续实机动作继续独立跟踪。当前部署仍为已确认 epoch48；普通蜂窝数据关闭，无新短信、电话、分区或关机试验。

## 接手位置

- `crates/core/src/setup_ap_control/station.rs`：授权、试连/取消/查询、私有 intent 和恢复。
- `crates/core/src/setup_ap_control.rs`：父 AP 会话、网络锁、能力和 Core 命令入口。
- `crates/core/src/authenticated.rs`、`crates/wire/src/auth.rs`：既有认证分派与有限协议。
- `crates/setup-ap/src/channel/station.rs`：Root 请求、摘要与回执验证。
- `crates/setup-ap/src/channel/files.rs`、`crates/setup-ap/src/executor.rs`：固定槽清理与显式可配置能力。
- `crates/setup-ap/src/executor/configuration.rs`：已有 Root 试连执行器；本次不增加 Save。
- `crates/core/src/setup_ap_control/station/tests.rs`：上述 12 项 Core 隔离测试。

下一步 AP-02 使用一个清晰的配网页面和单事务进度，刷新只查询；最终保存依赖 AP-03，不能把试连 Ready 当作已保存。
