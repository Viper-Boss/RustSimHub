# 受管 SDK 与实际 Root 提供者候选

这是已授权的隔离实现与审查材料，不启用默认 Core/Root 服务，不安装账户、策略、
密钥、nft、路由或程序。Root、协议和依赖选择仍需 Viper-Boss 精确审核；硬件、
真实 NetworkManager、systemd、ARM 运行和部署不能由隔离容器结果代替。

`tools/nkn-engine/rootguard` 实现实际 Root 程序，Core 的
`root-provider-candidate` 实现真实 `RootPort`/`EnginePort` 及原三引用监督器装配。
`Controller::new/snapshot` 和无活动 run 的 `drive` 不打开 Root/SDK、不创建身份。
只有原 Auth 的真实近期本地 Owner 可以取得 `NknRootAuthority`；Client/NAS、
普通 NKN token 和网页字段不能签发。保留原登录的认证及 session 截止，实际
Owner/realm/line 变化不继续采纳。没有自动续授权。

Root 固定读取 `/etc/rustsimhub/nkn-policy.json`，必须由安装器以如下字段顺序
输出紧凑规范 JSON（可有最后换行）；未知/重复/逃逸别名或错误格式拒绝：

```json
{"format":1,"source_commit":"<完整40位SDK源码SHA>","engine_sha256":"<完整64位程序SHA256>","core_pin":"<实际Core公钥64位hex>","statistics_public_key":"<独立统计公钥64位hex>","interface":"wlan0","seed_rpc":["https://<获审固定seed主机>:<端口>"]}
```

接口只允许 `wlan0` 或 `usb0`；HTTPS seed 由安装审核决定，不假设上游 HTTP seed
自动支持 HTTPS。SDK 限制拨号器拒绝代理、环境 DNS、私网、保留地址、嵌入地址和
其他 RPC 地址；真正出口还由 Root 的 nft UID 规则限制。DNS 仅来自实际当前 boot、
三十秒内新鲜且唯一的所选网络设备状态，最多三项。nft 在所有允许规则之前验证
实际输出接口，只允许指定 DNS 的 TCP/UDP53 与所选接口的有限公网 TCP 范围；
其余该 UID 流量丢弃。不存在蜂窝、其他接口、LAN 或通用代理 fallback。

固定程序 `/opt/rustsimhub/selected/nkn-sdk-engine` 必须匹配 Root 文件哈希、真实
`--version` 源码 SHA、Go1.27.3、SDK linked 和非夹具标记。Core 还比较策略公钥与
同一实际 Auth 设备公钥。Root 固定 NSS 六角色 Core/Edge/forwarder/BLE/media/NKN
必须存在、无 UID 复用、无别名；网络守护为 Root，永不充当 NKN 低权来源。NKN 无
其他补充组。模板没有占用数值 UID；缺失或不一致即拒绝，不自动创建/修复。

独立传输种子只从 `/var/lib/rustsimhub/nkn/transport-seed` 读取，Root0600、单链接、
恰好32字节，不能复用 Core/统计身份。没有 status 生成种子。Root 用一次性匿名
管道 FD3 和直接父进程 Unix FD4 发凭据及有界 BOOTTIME 租约，固定受检程序 inode
通过 FD5 执行。Root 服务必须在 exec 前继承 NoNewPrivileges；程序检查所有 Go
线程，不能用一次 Go goroutine 的 prctl 冒充全进程。子进程真实/保存 UID 与组
全部降权，capability/ambient 为空，没有 TUN/NET_ADMIN。

Root 从自身 unit cgroup 移入 manager 子组后启用 cpu/memory/pids 控制器；SDK
单独 engine 子组内存128MiB、32任务、20% CPU。启动共享十秒；Root 和原监督器
分别约束最多三次、三十秒窗口、至少五秒间隔，绝不重获或延长原最长三百秒租约。
Root 核验原 PID starttime、程序 inode、UID/capabilities/NNP/cgroup；SDK 实际
OnConnect ACK 才变 Ready，这仍只表示节点连接，不表示 Core 或媒体成功。

Root 私有 socket 固定 `/run/rustsimhub-nkn-root/root.sock`，仅实际内核 Core UID；
82字节请求及87字节回复逐字段绑定 generation、原到期、操作 nonce、固定策略
哈希、实际受管 PID。Root 持久分配32位代次，四字节0600文件经 pending/fsync/
rename/父目录fsync，损坏、旧 pending 和耗尽拒绝。新 Core 不重置代次；初始0只
用于注册及原 nonce 的查询/清理，不可启动引擎。连接最多4096条有限控制记录、
最多一个拥有者加一个恢复查询。原 Core 连接 EOF 只清理原范围；未知 ACK 不重发
start。只有原 child 实际 wait/reap、整个 cgroup 为空、原 nft 规则与哈希仍匹配且
删除回读成功，才报告 Released。篡改/超时/未知完成保留 Unknown，不能停任意 PID
或删除别人的规则。服务退出保留 socket/lock/evidence，自动重启不能重采旧范围。

Core 的固定普通入口 `/run/rustsimhub/nkn/core.sock` 需要单独安装目录：实际 Core
为 owner、NKN 主组为 group、模式2750，socket0660。`nkn_listener::serve` 只供显式
获准且已经 Ready 的 Remote 装配调用；不会创建目录或默认监听。实际来源先过
全进程共用的单槽 registry，再把内核 PID 和 Core 新连接 nonce 交真实 Root
producer 核验。监听重开不能丢掉仍在工作的原槽。原 Noise/Ed25519/membership/
正常 Auth/media 派发保持。Root 的观察只在一秒内采纳，过期、EOF或父权限失效
关闭原会话；无新 Root 成功时不恢复旧会话。Core/media 每次还检查只能收紧一次
且 false 永久生效的父 fence，使用 Weak 关联避免 Auth 与会话形成引用环。

ManualEngine、Remote、Statistics 三引用仍由原监督器管理。关闭 Remote 立即
使其原 fence 永久 false，而 Manual/Statistics 可继续持有 SDK；重开创建新的
引用，不能使旧会话恢复。最后引用释放才停进程。普通 SDK stdio 保持原只读状态/
关闭契约，未知 Owner/路径/计数/DHT 不造值；其他消费者不得持有全局 stop 管道。

隔离验证入口 `root_kernel_fixture.py` 使用固定 Python 容器、独立 cgroup/netns，
实际 Root 程序、真实低权 Rust Auth/监督器和生产 SDK 程序、nft1.1.3、内核接口与
进程退出；9.9.9.* 仅为该 `--network=none` 的自有命名空间地址。包含三引用、原
租约/EOF、固定 DNS/接口/私网拒绝及规则篡改保持 Unknown。它不验证真实 systemd
单元或物理 NetworkManager，模板必须在主线安装时另行核对。
