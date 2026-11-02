# A13/A15/A22：真实受管 SDK 引擎候选

承接已提交审查的 [PR178](https://github.com/Viper-Boss/RustSimHub/pull/178)，
不重写或重交已合并的 186/125/66 行补丁及实际 SDK 只读状态适配。
本包新增独立完整程序 `cmd/nkn-sdk-engine`，默认原 `nkn-engine` 仍离线。
用户已明确转交 Root/密码/Core/媒体/SMTP 的隔离实现，合并、安装和验收仍由负责人执行。

新增 managed 补丁只在独立验证后的派生 SDK stage 应用，固定前后哈希和完整源码树；
保留原来源、许可及版本。实际 SDK 限制测试复现旧版缺少 JSON 字段的可达 panic，
并验证 managed 模式拒绝短密文、错误 nonce、非有限启动配置，禁止内部自动重连。
另修正实际 block-hash 读写、递归读锁及 challenge/close 通道并发边界。
补丁没有把旧依赖通告视为已消除，既有 x/crypto、x/sys 等处置仍须审查。

真实启动仅接受直接 Root 父进程 Unix 内核凭据和一次性匿名凭据管道，验证实际程序
哈希、空 capabilities、NoNewPrivs、独立传输身份、有限 generation/nonce/BOOTTIME
租约。公开 stdio 保持原 status/stop，没有启动参数、秘密、URL、UID 或任意路径。
SDK 构造与实际 OnConnect 共用十秒，租约最多三百秒；Root/stdio EOF、过期或信号
关闭拥有的 IO。SDK 就绪事件只表示节点连接，未知 Owner/进程/路径/计数/DHT 不造值。

固定普通 NKN Core socket 的真实 relay 检查内核 Core UID，单来源/单 FD、四次原
握手和有界原密文记录，不提供通用 LAN 代理。重复 SDK ID 不进入 Core，错回复、
取消和不确定结果关闭来源，不恢复旧握手或重发操作。实际 Auth/Noise/成员/媒体
权限仍由 PR178 中原 Core 路径校验，SDK encrypted 字段不当成业务授权。

初步验证：四项实际 SDK 新测试、七项 managed 测试及原 SDK/race 回归通过；
默认模块 bridge/protocol/managed 全套 race 和候选 vet 通过。隔离容器中实际
Root 父进程→UID1002 SDK 子进程十二场景通过：ready/status/stop、Root/stdio/
WebSocket EOF、启动期 EOF、前后畸形及超大 WebSocket、租约过期、错程序 hash、
身份复用、错 generation。全部实际回收，最多一次 WebSocket，无 SDK stderr；
三种预检拒绝的网络调用为零。没有公网、链上、设备或真实 Core 业务执行。

实际运行代码提交 `fcbd46520dcaca1e241abc41ebc448e161a86bd7`，Root/Auth/Core
提交 `a2f1acc885919012e3c071930c72135fa44e7af1`；固定构建源码
`60d149423814c14fcd4bbab04be32f095d271876`。SDK 与 Root 各自 native/ARM64
两轮共八次完整程序构建、四次独立缓存比较逐字节一致。SDK native11346044B/
`e3ecb38f4552dbc9d36d2a4ca91560f35ec6d19c965b79462d1f95ed432a0fdf`，
ARM10616956B/`e742265c286942eeff8b1cf646ab5920fe15bf6bcb76483fd4378cca4247811e`。
Root 两架构与全部八份哈希见
[公开完整程序清单](../dependencies/nkn-managed-programs-candidate.json)。ARM 未执行。

固定源码37项 Go/实际 SDK/race 通过，默认模块全套 race/vet 及候选 vet 通过；
六 Rust 组件683通过/0失败/3个普通套件专用忽略。实际 SDK UID 十二场景已重跑，
Root/Rust Auth/三引用/生产 SDK/真实 cgroup/nft 五场景已重跑，源码 tracked clean。
源码扫描23项模块通告的受影响包均不在该两架构完整程序实际闭包，未将其视为
模块修复或审批。v1.9 binary 扫描报告了真实 pclntab/编译闭包中不存在的 SSH
函数，单独保留工具限制，不将输出当可达证据。实际许可证逐文件哈希匹配，保留
[完整 notices](../nkn/managed-programs-NOTICES.txt)，16个既有外部模块加本仓库
模块，没有新增外部模块；依赖/生产审批仍未代签。

真实 Root/独立低权/cgroup/nft/原租约对 Core admission 的证明已由接续包实现，
见 [Root 交付](member-a-managed-root-provider.md)。继续 Core 公共候选控制 provider、
A22 采集映射、A29 queue4/worker5/七阶段兼容、BlueZ 与 WASM/CSP/PWA 装配及
A30 组合；没有将应用 IP 过滤当作 Root 策略证明。
没有安装账户、服务、密钥、路由或程序，没有自行合并、部署或修改全局进度；
整体实际验收仍为 81.5%、20/169。
