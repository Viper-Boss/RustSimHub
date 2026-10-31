# D24 BLE 服务材料：独立组和通知 FD 的待审提案

这是一份只读提案及最小失败样例，不是安装器、生产 Root 格式或已批准的 UID 分配。`firmware/system/ble/material_plan.py` 无参数读取同目录四个有限模板并输出 SHA-256 清单；始终 `production_ready=false/install_permitted=false/default_enabled=false`。不创建角色/目录/socket，不执行服务、重载 D-Bus、读取凭据或连接设备。

旧 #53 的 `00f87779ff79ca680433e0a0b5ed35c7d19437c1` 原作者 Viper-Boss 保留，通过 cherry-pick -x 引入。该版本会接受 `SupplementaryGroups=rustsimhub-ipc`，与目前 BLE 不加入通用 IPC 组的边界冲突；最小探针在修正前实际失败。本人修改严格拒绝旧组、旧通用 Core socket 路径和 sysusers 的组成员条目。不更新、关闭旧 PR，也不改其他成员源文件。

## 待审核接口

| 项 | 有限候选 | 仍须核验 |
|---|---|---|
| BLE 角色 | rustsimhub-ble，UID/GID 候选 982 | 未预留、未批准；实际本地及 NSS 名称/数字/别名/成员表 |
| 附加组 | 空；不加入 rustsimhub-ipc/bluetooth/audio/其他服务组 | `SupplementaryGroups=` 只重置单元配置，不能清除 NSS 已授予的附加组 |
| 专用目录 | /run/rustsimhub/core-recovery，owner Core/group BLE，2750 | 这是新目录提案，未创建；Root admission、父目录、命名空间和 Core 实际绑定 |
| socket | 专用目录中的固定恢复端点，0660 | 文件名/创建和清理交由恢复 IPC/Root 接口审核；不能复用通用业务 socket |
| NSS 兼容性 | 现有 setup_ap_install.identities 不变 | 只对合成表测试：独立 BLE 角色可通过；加入通用 IPC 组仍拒绝。不是生产 NSS 检查 |

提案 `format=2` 只属于这份未安装检查材料，不修改应用 wire、数据库、Root admission 或发布格式。现有读取器不提供 format1 兼容兜底，也不自动迁移旧模板。UID/路径或权限若经审核变化，须另行显式修改对应模板和拒绝样例。

Core 独占目录写权限，BLE 仅借主组搜索目录和连接 socket；Core 仍以真实 SO_PEERCRED、私有连接 nonce 和 Auth capability 作授权。组可连接不等于获准执行恢复命令。#125 的真实不同 UID 正例是在隔离容器完成，不证明这个生产目录、NSS 或服务已经可用。

## D-Bus 候选许可

- 首先拒绝 owning well-known names、任意发送/接收和所有输出 signal，之后只开放列出的规则。
- Broker 只放行 Hello/GetNameOwner/GetId/AddMatch/RemoveMatch，固定 broker 路径、method_call、0 FD。不允许 ListNames/RequestName 或任意接口。
- 仅 `/org/bluez/hci0` 的四项 GATT/广播注册撤销；PeerLease 查询只需 Properties.GetAll。动态 device 路径仍必须由 Rust 私有 owner/适配器/Connected 历史核验，静态策略不能表达该证明。
- 输入方法有限到固定 ObjectManager/service/advertisement/四个 characteristic 路径，Properties 只读，无 Set。AcquireNotify 仅 AuthExchange/Result；所有输入调用/回复/信号均 0 FD。
- 输出 method_return 仅向 BlueZ 的所请求回复，至多 1 FD；error 为 0 FD。D-Bus 回复没有原方法路径可匹配，实际 AcquireNotify/单 peer/一次 FD 的验证仍靠 #122 Rust 实现，不把策略当作业务权限。
- 仅 ObjectManager 的固定根路径 InterfacesAdded/InterfacesRemoved 元数据 signal 可输出；**无 PropertiesChanged/Value 输出许可，不走全局通知回退**。StartNotify/StopNotify 仅可到达并返回不支持；这不能证明真实 BlueZ 会拒绝 CCC，见 #117 的上游失败回退证据。

真实私有 dbus-daemon 已解析整份候选规则（包括 max_fds），并验证真实 broker GetNameOwner 成功，ListNames/RequestName/Properties.Set/错 member 失败。测试仅把 user 名替换为当前低权测试 UID，未修改其他许可。**这份16项单UID检查不能证明跨UID/FD或组合系统策略的最终权限。** 后续[真实UID策略试验](service-policy-kernel-uids.md)已用未改动候选策略/合成BlueZ/libdbus验证unique owner及SCM_RIGHTS数量边界；它仍不是实际BlueZ或Rust GATT跨UID集成。 生产已有 group/user/mandatory 策略顺序也须主线核对。

## 服务边界和关闭

模板没有 Install/WantedBy，双 AND 条件，默认禁用；实际 `--service` 程序尚未交付。空 capabilities/NoNewPrivileges、仅 AF_UNIX、私有网络/IPC/设备、closed DevicePolicy；通用 Core 运行目录及业务数据不可见。读取专用 socket 路径的 bind、凭据装载与 mount sandbox 只做离线解析，未实机启动。MemoryMax=48M/TasksMax=16/CPUQuota=10%/25秒停止为候选预算，不是压测结论。

没有 ExecStop shell 或任意 Root 清理器。真正启停必须由 host 保留注销 future、超时后关闭专用 D-Bus 连接、失效 peer/FD/恢复 channel；进程退出不能证明空口广播已清理，D-Bus FD send Ok 也不是浏览器/业务收到。Core 单元保持独立；本包不改 Core 依赖、不安装 BLE。真实 RF 清理、服务退出/cgroup、缺 HCI 降级、BlueZ 重启须后续集成验收。

## 重现与边界

```sh
python3 -B firmware/system/ble/test_inspect.py -v
python3 -B firmware/system/ble/material_plan.py
git diff --check
```

16 项通过、0 失败、0 跳过；含真实临时 systemd root 的离线解析（合成可执行文件从未执行）、真实私有 Unix bus、现有 Root NSS 校验函数、严格未知/重复/扩权字段拒绝及 regular-file/大小/链接/并发变更失败路径。未改 Rust，未重跑整仓 Rust；不借用其他分支的 950 测试作为本分支结果。许可证 AGPL-3.0-only，原作者/提交来源保留，Codex 辅助。

D24 仍未完成生产角色/Root/服务安装和实机预算验收。本包完整步骤新增 0：21/26 离线包已提交、剩 D22–D26 五步；固定 P0–P7 约 80%，离线提交不等于完整验收。下一步是逐 UID 的私有 D-Bus/FD 许可验证、已批准的角色及集成 host；不抢 A 密码实现或代替实机验收。
