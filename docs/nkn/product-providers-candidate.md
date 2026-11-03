# 实际 SDK 状态、控制与恢复产品候选

本包承接已由负责人合并的 PR178/184/187，不重复实现其密码、SDK、Root 或 SMTP
功能。所有新增运行入口均默认关闭；没有安装账户、启动设备服务、签名、迁移真实
数据、对公网发包或发布配置器。代码候选和设备验收分开记录。

## 明确选择候选入口

Core 编译特性 `product-providers-candidate` 包含原 Root/普通 NKN/安全会话候选。
启动前缀 `--enable-product-providers-candidate` 接在原命令之前，启用同一个 Core
的 NKN provider。可随后加 `--enable-secure-recovery-candidate`，再接原业务启动
命令。第二个前缀仅显式打开既有 schema9；不执行 schema6/7→9 迁移。恢复 Wi-Fi
业务仍使用原来的 provisioning actor/journal，原启动方式未提供该 actor 时不能
把加密握手成功当作 Wi-Fi 功能可用。默认编译和默认启动方式保持原行为。

BLE 编译特性 `product-host-candidate` 提供单独程序 `rustsimhub-ble-recovery`，只接受
`--serve-fixed-candidate`。它读取可信安装账户，在独立系统总线连接上固定原 BlueZ
唯一所有者和 hci0，不修改 Power/Pair/Trusted。一个 epoch 只接收实际对象树中唯一
连接的 Device1，再由原 PeerLease 验证。关闭、所有者替换、断线或错误均结束此轮，
没有自动跟随新 BlueZ、重新广告或重新连接 Core 的循环。

Core 连接在第一条实际 Auth 帧才创建，广告和用户选择期间不消耗 Core 的十秒握手
截止。取消尚未轮询的任务也永久结束该 epoch；断开能立即关闭正在工作者持有的同一
Core FD。BLE 只转发有限不透明字节，不持有设备私钥，不颁发 Core 权限。

## 安装前置和固定路径

账户 UID/GID 来自可信 NSS 安装文件，不在本包分配数字。Root helper、SDK、固定
策略、独立 seed、cgroup/nft 和出口仍执行 PR184 的检查。

| 路径 | 可信边界 |
| --- | --- |
| `/run/rustsimhub-nkn-root/root.sock` | 原 Root helper；Core UID、原 generation/nonce/policy/deadline |
| `/run/rustsimhub/nkn` | Core 所有、NKN 主 GID、2750；Core 建立 0660 socket，不覆盖旧 socket |
| `/run/rustsimhub/nkn/core.sock` | 实际 SDK UID/PID 与原 Root ready-child、原连接 nonce 一起准入 |
| `/run/rustsimhub/recovery` | Core 所有、BLE 主 GID、2750；与普通 NKN 分开 |
| `/run/rustsimhub/recovery/core.sock` | 原 SourceGuard 只接受实际 BLE UID；同 Auth/Store 的实际恢复会话 |
| `/etc/rustsimhub/nkn-membership.bin` | Root 所有、Core 主 GID、0640、单链接普通文件、可信父目录 |

membership 文件是固定 93 字节的私有安装材料，不是网络或浏览器 JSON：

| 字节区间，左闭右开 | 内容 |
| --- | --- |
| 0–5 | `RSMK` 和版本字节 1 |
| 5–21 | 原 membership 实现的 16 字节 network 输入 |
| 21–53 | 32 字节私有 secret |
| 53–61 | 大端非零 u64 credential generation |
| 61–93 | 当前 Core 的完整 Ed25519 公钥 |

本包不生成或提交生产材料。只有新的明确 Remote ON 在当前本地近期 Owner 检查后
读取材料；读状态不加载材料或启动 Root。替换、删除或权限变化会撤销原 key/session，
检查间隔 250ms；原 listener 退出后不自动重开，必须 OFF 后再明确 ON。旧 Remote
引用的撤销标记不会因新 ON 复活，原 Root 授权截止也不会因重新引用而延长。

## 实际状态和控制

Root 策略可显式增加 `status_protocol:1`；省略时旧 canonical 策略字节保持，Core
不试探旧 Root 的新操作。op6 使用独立、实际 Core UID 准入的观察连接，只读同一
原 scope。丢失或超时只结束观察，不能关闭 Root owner FD、停止共享 SDK 或续期。

Root 只查询自己保留的 SDK stdio，校验 UUID、完整 T5 字段、顺序和 SDK 链接状态；
200ms I/O、每秒最多一次、每子进程 1024 次，错误使此 reader 不再重试。原 SDK
process 采用 4096 帧预算，默认离线前端仍为 32。持久 process epoch 在明确 SDK
启动时分配；它与原 Root 授权 generation 分开，读状态不分配新 epoch。内核采样
年龄保守传递到 Core 和页面，不能把旧缓存或 HTTP 到达时间当作新样本。

T5 中 enabled 来自实际三个引用，service 来自原 Root ready PID/cgroup 证据。
SDK 不提供的 DHT、链路、字节数、RTT、节点数保持 unknown/null；未用 0 冒充未知。
关闭一个引用不等于进程退出；只有原 scope 的 reaped/released 和监督器 Disabled
同时成立，才显示已确认退出。普通 NKN 成员、统计计数和 Root 授权不是同一证据。

原 Auth RPC 新增 `nkn_status`、`nkn_request`、`set_nkn_enabled`；精确字段以 wire
类型为准。读和查询要求当前 Owner/system.manage，变更还要求近期 Owner；任何 ON
仍需当前本地 Root authority，远程 NKN token 不能借已有运行实例增加 Root 引用。
控制用 Core instance、单调 revision、独立 UUID 和 64 条回执限定；先登记 Unknown
再执行可能的 Root I/O，旧 UUID 即使被淘汰也不能用旧 revision 重放。监督器与
变更通过同一动作锁串行化，listener 不再另开第二个 Root 轮询器。

Edge 页面手动读取，不自动启停或刷新授权。变更前先持久写入并读回本地意图，收到
精确回执并归档读回后才清除。Unknown/not_found/换 Core/断线不自动重发，失败和
陈旧读数立即保持未知，旧 sequence/generation 和退出登录后的迟到回复被拒绝。

## 恢复配置器制品

`tools/build-recovery-configurator.py --output work/NEW --wasm-bindgen PATH` 实际执行
锁定 Cargo WASM 构建、wasm-bindgen 0.2.129 和静态组装，不发布。原普通 site builder
保持无后端；只有此明确候选包含 production entry、原安全 backend 和生成的 WASM。
每文件 512KiB、总计 2MiB 预算保持。CSP 只在此制品允许 `wasm-unsafe-eval`，不加
通用 `unsafe-eval` 或外部 CDN；CSS 使用内容哈希。发布方仍应设置相应 HTTP 安全头，
不能把 meta CSP 当作服务端 frame-ancestors。

首次加载只初始化静态密码模块。独立非导出 CryptoKey 的创建、原管理入口批准后的
回执保存和设备选择均需明确操作。pin/realm/授权编号/代次用于定位原登记记录，
本地填入这些字段不能批准 Core 权限；真实设备仍检查 grant、私钥、签名、期限和
撤销。浏览器不从同名设备推断信任，不导出私钥或把密码写入持久记录。

SW 仅缓存组装的明确静态资源，包括 WASM。原更新前检查保留：有会话、未确认请求、
未知存储或其他配置器窗口时不强制激活、刷新或重新提交请求。第一次离线没有已
缓存的资源仍不可用。手机、实际 RF、生产签名和正式部署仍由负责人安排。
