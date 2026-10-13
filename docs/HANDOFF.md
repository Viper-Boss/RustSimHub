# RustSimHub 接手入口

最新用户指令（2026-10-14）：把持续自主推进目标提高到整体约 75%，取代此前约 60%。当前实证约 50%；百分比分母和 P0—P7 固定权重不变，不以扩大文档或跳过验收提高百分比。继续工作，不因保存检查点停止。


新增 VALIDATION-RTC-P3D.md：棒子真实 UDP/ICE/DTLS/SRTP 合成回环、NAS 与棒子五秒双向编码静音通过。没有打开 PCM、拨号或新增短信。
native aff5dcd 已部署 /opt/rustsimhub/dev-rtc-p3/，self-check 双向 50 帧；LAN 各发 250、收 250/249，解码静音只有 1 LSB 量化残差。
最新源码补充证书替换失败、信令输出原子发布和一秒媒体断流关闭，尚须下一次原生构建验证。root 2c69ff9 的 125 项、默认 production core 构建通过，prod/development 分开；P6 实机签名应用验收仍未加点。
不要重跑旧电话 once；用户持续允许 10010，但新的电话测试要用独立标记且确保挂断。约 75% 长任务继续。


当前整体约 **50%**，见 PROGRESS-SCALE.md。用户要求每次最终回复报告整体完成百分比；
继续按固定里程碑估算，并注明实机验收边界，不把代码完成当作整机固件完成。

最新开发：见 rtc-and-application-update.md。新增 LAN DTLS/SRTP/Opus 与签名应用选择器，尚未全部实机验收，整体约 50%。
主线最新 Git 9b34276；当前还有媒体私有文件非阻塞、ELF/pin 和证书失败测试修改待提交。125 workspace / CI 8f087a2 通过；native aff5dcd 的 9 项测试通过，release 仍在构建。
两个自有 NAS 构建器当前运行；先查 liveness。首个新 root 构建因 Windows CRLF 源包与 Git blob 不一致，只保留测试，不可部署归属到 HEAD。现 tools/package-source.py 精确打包 Git blob；最新修改提交后，重建两个产物并验证 hash。
下一步：完成原生加密回环、NAS/棒子真实 LAN、获准 10010 的静音上行/远端解码统计；随后签名应用 stage/confirm/失败回退实机、Wi-Fi 事务和兼容迁移。不要在保存本检查点后停止长期目标。

当前长任务目标：持续自主推进至约 75%，仍在进行；不要在一个小检查点停止。浏览器/Wi-Fi 只读通过，继续 Opus/WebRTC/网络管理/兼容或系统交付。
新增 VALIDATION-BROWSER-P2.md：真实浏览器配对、签名登录、短信正文与分页、重启会话拒绝/新登录、只读准备拒绝通过。
Wi-Fi 浏览器通过仅回环 QA 代理严格校验上游 TLS，未绕过浏览器 TLS 警告或安装 Windows CA；正式直接浏览器信任仍待完成。
当前 app.js 的退出/在途请求代次修复在 QA 实测通过；最终静态 binary 构建和不同 UID 实机烟测通过。Opus 静态原生编译及棒子合成验收通过，见 VALIDATION-CODEC-P3C.md。
本轮新增离线 CMake/Ninja 编译依赖只进入自有镜像，opusic-sys 锁定的 Opus 1.6.1 将静态交付；不要给宿主 NAS/棒子安装编译包。
继续前检查 own systemd 单元/QA 代理与构建器的实际 liveness；文件指针或旧日志不能当作仍运行证据。结束时停自己的开发窗口和回环代理。
先读 edge-and-network.md、VALIDATION-EDGE-P1.md。检查点 a00bd9b，120 workspace + CI Clippy + ARM64 release 通过。
棒子 `/opt/rustsimhub/dev-edge-p1/` 的三程序均自报源包 SHA 78bfc723...42c8a6，完整 hash 在验证文档。
新增锁定 UID rustsimhub-core / rustsimhub-edge 与 IPC group；不修改旧服务账户。HTTP 实测不同 UID，owner key 只留可信 Root 操作端。
所有本轮临时服务已停止，没有新增开机服务或网络写入。下一阶段不要把静态页面及 Node WebCrypto 测试当作实际浏览器交互已完成。
NAS 构建曾受旧解压 mtime 影响复用旧 release，现 `tar --touch` + compile-time source hash；部署前必须自报匹配，不只核对源包文件。
先读 VALIDATION-MEDIA-P3B.md 和 media-authorization.md。core/media 接线、两秒/五秒真实 PCM、正常 core 退出撤销通过。
目前上行只送静音，media_available=false；不能称已完成双向通话。HTTPS/网络只读已验收，浏览器与网络写流程还未验收。
用户 2026-10-13 明确持续授权本项目拨打 10010；这条新授权取代下方历史“下一次需新授权”的说明。其他号码/新短信仍需明确指令。
本阶段 11 次已使用的 once 标记全部保留，各独立库一个 dial 和一个 hangup，零短信操作；不可删除标记或自动重试 Unknown。
棒子 `/opt/rustsimhub/dev-media-p3b/` 是本轮实际测试目录。最新五秒稳定 PCM native 哈希和私有证据位置在验证文档。
完整备份/恢复尚未闭环，仍禁止替换 rootfs/boot 或写 NV/校准。

先读 PROGRESS.md、PROGRESS-SCALE.md、audio-foundation.md、VALIDATION-AUDIO-P3A.md、call-control.md、VALIDATION-CALLS-P3.md、sms-send.md、VALIDATION-SMS-SEND-P2.md、observer-events.md、VALIDATION-OBSERVER-P2.md、sim-inbox.md、VALIDATION-INBOX-P2.md、VALIDATION-RECOVERY-CLI-P1.md、recovery-clients.md、VALIDATION-AUTH-P1.md、auth.md、VALIDATION-STORAGE-P1.md、storage.md、VALIDATION-P0-P1.md、HARDWARE-BASELINE.md、architecture-foundation.md，
再按 development-plan.md 的阶段顺序继续。GitHub：
https://github.com/Viper-Boss/RustSimHub（私有）。

## 私有资料

原开发机仓库根目录 .private/ 包含 nas.json、device.json、原始日志。
这些文件被 Git 忽略。NAS 密码不写进任何可提交文件。
device.json 引用旧工程维护密钥和固定 known_hosts，通过 NAS 转发至 USB 设备。
在新机器接手时需要安全取得这些材料；不要关闭 SSH 主机密钥校验。

.local/python-deps 为本地 Paramiko，.local/cargo 为 Rust 缓存。
原开发机 Windows 缺少 MSVC 链接器，优先使用已建立的 NAS ARM64 构建环境。

## 常用操作

从项目根目录运行：

```powershell
python tools/remote.py --config .private/nas.json --script tools/nas-baseline.sh --output .private/nas-baseline.log
python tools/remote.py --config .private/device.json --script tools/device-baseline.sh --output .private/device-baseline.log
python tools/remote.py --config .private/nas.json --sudo --script tools/nas-builder-status.sh
python tools/package-source.py
```

NAS 构建目录 `/mnt/docker-data/rustsimhub/p0`；容器 `rustsimhub-build-p0`。
源包只有 Cargo 清单、锁文件、许可证、crates 与 native/audio-alsa 源文件。容器以 UID/GID 1000 运行，
限制 2 CPU、2 GiB 内存；不提供设备、host network 或主机控制 socket。
旧 NAS 容器和旧棒子服务不属于此新仓库的可删除资源。
重新构建前先确认该构建容器已停止、保留 build.log 及产物，再仅移除
`rustsimhub-build-p0` 容器（不加 -v，不删除源目录和缓存）。
随后上传新的允许列表源包并执行 tools/nas-start-build.sh；脚本会拒绝复用既有容器名。
构建镜像已固定 digest，避免未来同名标签变化导致不可追踪的构建环境。

boot 备份位置：本机 `.private/backups/boot.img`，NAS
`/mnt/docker-data/rustsimhub/private-backups/p0/boot.img`，棒子临时副本
`/var/tmp/rustsimhub-p0-private/boot.img`。三份均为 64 MiB，
预期哈希在 firmware/board.toml；本机与 NAS 已复核。
不要重复执行 tools/device-backup-boot.sh：它会拒绝覆盖既有备份。
完整恢复备份仍未完成，boot 单独备份不能恢复用户数据或整个系统。

## 实机烟测边界

tools/device-smoke.sh 将 `/var/tmp/rustsimhub-core-p0` 安装到
`/opt/rustsimhub/dev-p0/rustsimhub-core`（避免被 PrivateTmp 隐藏），
启动 `rustsimhub-probe-p0` 临时 systemd 单元，最长 300 秒。
仅允许旧系统 nassimhub 用户读取 health/capabilities，完毕停止单元。
没有安装开机服务。异常退出后可执行 `systemctl stop rustsimhub-probe-p0`。
原 P0 ARM64 release 产物留在本机 `.local/artifacts/rustsimhub-core`、NAS 的
`history/p0-first/rustsimhub-core`、棒子 `/opt/rustsimhub/dev-p0/rustsimhub-core`。
哈希和实测资源值见 VALIDATION-P0-P1.md。

SQLite 持久化/故障恢复已验证，实现见 storage.md。设备身份、公钥配对、短期会话和
core 鉴权基础也已验证，见 auth.md；禁止添加未鉴权的业务写接口。
存储诊断命令只在 dev-fixtures feature 下提供，测试只使用合成数据。
tools/device-storage-smoke.sh 使用 dev-p1 独立产物及随机私有临时目录，
不会读写原项目业务数据库。测试库留在棒子上，可用于后续排查。
NAS 官方精简构建镜像没有 Clippy；Clippy -D warnings 在 GitHub CI（Rust 1.98）执行。
旧存储 P1 产物在本机 `.local/artifacts/rustsimhub-core-p1`、NAS `history/build-*` 归档、
棒子 `/opt/rustsimhub/dev-p1/rustsimhub-core`，哈希见 VALIDATION-STORAGE-P1.md。
后续重新构建使用 tools/nas-reset-stopped-build.sh：校验已退出、标签、镜像和挂载，
归档后只移除本工程构建容器；保留缓存及全部旧服务。
上一身份包源码检查点为 5389eb8；Store 已绑定 --serve-authenticated 的 daemon 生命周期。
旧鉴权 P1 产物在本机 `.local/artifacts/rustsimhub-core-auth-p1`、NAS `history/build-*` 归档，
棒子 `/opt/rustsimhub/dev-auth-p1/rustsimhub-core`。
工具 tools/device-auth-smoke.sh 仅使用新的随机私有目录与合成 owner/client；不读取旧数据库。
它以 nassimhub 非 root 用户启动 rustsimhub-auth-p1 临时单元，最长 180 秒，测试后已停止。
异常时停止该单元，禁止把 fixture 的已知密钥用于正式设备 owner。
真实 owner 尚未初始化。simhubctl 及可信离线恢复已完成，最终用户安全设置 UI 还待开发。
恢复 DB 会隔离 owner；用原 owner 私钥及 --recover-owner 重新证明，不能删表重新 bootstrap。

上一轮源码检查点 7f69dd8；先读 recovery-clients.md 的命令、pin 与权限边界。
上一轮 core/CLI 在本机 `.local/artifacts/{rustsimhub-core-recovery-p1,simhubctl-recovery-p1}`，
NAS 当前 `src/target/release/{rustsimhub-core,simhubctl}`，棒子
`/opt/rustsimhub/dev-recovery-p1/{rustsimhub-core,simhubctl}`，哈希见验证文档。
tools/device-recovery-cli-smoke.sh 只生成新的私有合成目录，使用 FIFO 协调离线证明，不需要 Python/jq。
临时 rustsimhub-recovery-p1 单元最长 180 秒，最终已停止；异常时仅停止此项目测试单元。
SIGTERM 清理自身 socket；SIGKILL 后持锁确认旧 socket 不再监听才接管；就绪需真实 IPC 握手，不能只测 -S。
客户端重新批准要求 fresh proof 和精确 generation；不让旧 token 复活。
--backup-identity 保存独立明文私有 seed，拒绝覆盖；先加密/安全保管后才能作为正式异机备份方案。
NAS 构建还检查 workspace 默认 feature；CI 文件因 workflow scope 限制保留原配置，测试/Clippy 均通过。

Git HTTPS 偶尔被连接重置。当前机器 `.local/api-push.py` 是备用方式：只允许远端为
本地 HEAD 的父提交、比较创建树与本地树、不强推；GitHub 规范化提交时验证对象后同步本地 ref。
首次 bootstrap 和数据代码仍完整保留历史。不要复制任何凭据到源码。
tools/verify-build-source.py 检查当前源包与 HEAD 字节，tools/audit-source.py --revision HEAD
可检查已提交树的指定隐私模式；这不替代完整依赖/漏洞审查。

## 重要未完成项

上一收件箱 Rust 源码检查点 `fdaf4c2efbdb7c3b194fda88fc60e131dc217c61`，66 项与 CI Clippy 通过。
先读 sim-inbox.md 和 VALIDATION-INBOX-P2.md。`--serve-observing` 显式接入只读 modem，
`--serve-authenticated` 仍适用于纯鉴权/合成验证。schema v3 与旧 v1/v2 自动迁移；启动重置 SIM ready，
新进程/MM owner/SIM 变化重新隔离 epoch。PIN2/PUK2 在 operational 状态允许普通读取，不能自动解锁。
SIM 指纹使用派生用途 key；观察器不持有 signing seed，也不争用鉴权锁。

上一收件箱产物在本机 `.local/artifacts/{rustsimhub-core-inbox-p2,simhubctl-inbox-p2}`，
NAS 当前 src/target/release 与棒子 `/opt/rustsimhub/dev-inbox-p2/`。哈希见验证文档；core 是 dev-fixtures 开发包。
tools/device-inbox-smoke.sh 已通过非 root 实机读取已有 1 条完整入站短信、权限区分及撤销后正文/摘要拒绝。
`rustsimhub-inbox-p2` 临时单元最终已停止，最长 90 秒；私有随机目录保留。
detail.json/会话/原始诊断内容不能复制进 GitHub。测试不发送新短信，需要预先已有完整 modem 消息。

本轮已接信号订阅、重连/分批和逐对象错误隔离，以及可观察的对象 incarnation；继续处理历史归属和歧义复用；
本段为此前收件箱阶段记录；当时真实发送门禁尚未实现。当前实现见 sms-send.md。轮询 ready 仅代表 SIM 身份观察稳定，
不代表网络注册或电话/发送已验收。不要把内部 prepare/begin_submit 直接接到网络接口。

- 系统恢复入口、完整备份和升级断电验证尚未完成；禁止直接替换现有 rootfs/boot。
- domain 中短信对象仍是纯规则；当前真实 Send 使用 begin_authorized / bind_created / permit_send；禁止把 legacy begin_submit 暴露为接口。
- call grant 不是认证 token；禁止直接暴露给未认证远端使用。
- P0 的 MM 能力探测不代表来电/发短信/双向声音已验收。
- TLS/密码、最终用户设置 UI、生产系统单元、加密/异机身份备份与完整硬件恢复还未完成。

事件观察阶段 Rust 源码检查点 `31be30a805be626b913b099b3f76823378d518bf`，73 项与 CI Clippy 通过。
先读 observer-events.md / VALIDATION-OBSERVER-P2.md。基于唯一 owner 的监听任务持续消费，
关键通知取消读取；SQLite 单写者通过本地回调再次检查 fence 和监听器存活，不能对网络暴露回调或绕过身份授权。
目录最多 4096、每批 16、对象读取 1 秒/整轮 30 秒；正常路径每 10 秒修复，重连退避最多 30 秒。
SIM 身份变动或 owner/连接丢失清空绑定路径；Added/Deleted/目录消失或正文变化保留新的 source incarnation。
line_status 新增受 status_read 保护的 observer 计数，不包含路径和短信内容。

事件观察阶段产物本机 `.local/artifacts/{rustsimhub-core-observer-p2,simhubctl-observer-p2}`，
棒子 `/opt/rustsimhub/dev-observer-p2/`；core 含 dev-fixtures，私有模拟服务仅 cfg(test) 存在。
tools/device-observer-smoke.sh 通过实际订阅、已有 1 条短信读取、周期修复无错误、权限撤销与 SIGTERM 后 DB 重开。
临时 rustsimhub-observer-p2 最长 150 秒，最终已停止；空闲 RSS 6420 kB 样本不替代性能验收。
该阶段原下一步为 SmsSend。当前实现及验证见 sms-send.md / VALIDATION-SMS-SEND-P2.md；不能直接使用内部 legacy begin_submit。


## 当前短信发送检查点

Rust 源码 bb910b5280d8a442e4f37924c99484069a2e6614，79 项 / CI Clippy / NAS release 通过。
先读 sms-send.md 和 VALIDATION-SMS-SEND-P2.md。现有只读模式继续拒绝发送，只有 --serve-messaging 开放受 sms_send 保护的三条命令。
发送使用持久 begin_authorized / bind_created / permit_send 和唯一 owner/连接/SIM 门禁，不能调用 legacy begin_submit 绕过授权。
Unknown 永不自动重发，新进程/连接不按相同路径/正文推断旧发送成功。

当前产物 `.local/artifacts/{rustsimhub-core-send-p2,simhubctl-send-p2}`，NAS 当前 target/release，
棒子 `/opt/rustsimhub/dev-send-p2/`。均为开发产物，不是可刷写系统。
临时 rustsimhub-send-p2 与 rustsimhub-observer-send-p2 已停止；旧四个服务 active，网络正常。
非 root 收件回归通过；真实写模式为独立 root 临时单元，生产非 root 写权限仍需验证。

本轮用户授权的一次 10010 CXYE 查询已经完成，SENT 并收到私有回复。
棒子 `/var/tmp/rustsimhub-balance-query-20261013.once` 已存在，不能删标记或再次运行查询脚本。
私有目录定位在 `/var/tmp/rustsimhub-send-p2-evidence`；本机 `.private/carrier-send-evidence.tar.gz` 有 DB、测试身份、会话和回复，禁止公开。
后续可只读核验，不能把一次测试成功当作真实断电/换卡/投递报告等完整验收。

下一步推进通话控制/租约、音频 PoC；同步补齐短信历史/事件、保留策略、生产权限/TLS/systemd 及完整恢复与系统镜像。


## 当前通话检查点

最终 Rust 源码 73db36a42c1b88a0b789a0dcd21343b94d087b16，90 项 / CI Clippy / NAS release 通过。
先读 call-control.md / VALIDATION-CALLS-P3.md。--serve-call-observing 仅观察；--serve-telephony 显式开启通话及短信控制。
会话 scope + 完整 principal generation + 当前 SIM/owner/连接 + 服务端 30 秒租约共同保护操作。
控制动作在硬件调用前先提交单次许可，Unknown 不自动重试，优先挂断只操作自己持有的通话。
跨重启租约清空、观测隔离、未完成动作 Unknown；重启后外部/孤儿通话不安全自动接管或挂断。

用户授权的单次 10010 电话已完成：ACTIVE 后自动挂断，raw state=7、active_calls=0。
私有备份只有一条 dial、一条 hangup，两者 submitted/resolved，SMS 操作数 0。
/var/tmp/rustsimhub-carrier-call-20261013.once 已存在；这次电话及此前短信授权均已使用，不要删标记/重跑。
定位文件 /var/tmp/rustsimhub-carrier-call-p3-evidence；本机 .private/carrier-call-evidence.tar.gz 包含 DB/会话/随机测试密钥，禁止公开。
停止后 fixture 重开将观测置 Reconciling，同时保留最后原始 Terminated；不要因此推断还有活动通话。

当前产物 .local/artifacts/{rustsimhub-core-calls-p3,simhubctl-calls-p3}，NAS target/release，棒子 /opt/rustsimhub/dev-calls-p3/。
产物均为 dev-fixtures 开发包。rustsimhub-calls-read-p3、rustsimhub-carrier-call-p3 已停止；旧四个服务 active、网络正常。
非 root 只读和独立 root 写模式分别验证；真实来电/DTMF/音频、生产非 root 写权限及完整系统恢复未完成。

下一步 ALSA 双向 PCM 与媒体授权 PoC；再做 WebRTC、来电/DTMF和恢复管理，继续生产权限/TLS、网络与系统镜像。
涉及新的实机电话/短信测试需要新的明确目标与授权，不能沿用已经消耗的一次标记。

## 当前音频基础检查点

Rust 源码 `458a272372b9b1910af8e0ecbce7831a14700db7`，102 项 workspace + 4 项原生 ALSA 测试通过。
源码 CI `38348798372` 成功；workspace Clippy -D warnings 通过，exclude 的原生 crate 尚未跑 Clippy。
core/CLI 未变化；media_available 仍为 false，没有可刷写完整镜像。
先读 audio-foundation.md / VALIDATION-AUDIO-P3A.md；crates/media 的 Context 是可信组件快照，绝不能直接接客户端 JSON。
Authority/Ticket/Permit 未接 core 或 IPC，二进制只提供 --probe-readonly，不能打开真实 PCM。

原生 ALSA 库已实现固定 UFI003 的两路 CS-Voice holder + 16kHz 双向 PCM、nonblocking I/O、有限恢复和关闭；
精确 period/buffer 与同时打开时序仍需实际通话。路由初始化和 core/media 授权 UDS 是下一步依赖。
媒体跨进程时统一单调时基；绑定当前 Principal、真实 lease、ACTIVE 与 Binding fence，持续失效后必须关闭。
原生后端源码/锁文件独立于 workspace，禁止因此遗漏 native 构建测试。

专用 NAS 容器 `rustsimhub-audio-build-p3` 已退出 0；构建器/产物哈希见验证记录。
源码只读 `audio-src`，缓存 `audio-cargo`，产物 `audio-target`；只给构建容器装开发包，没有修改 NAS/棒子系统。
后续上传源包为 `/mnt/docker-data/rustsimhub/p0/audio-upload.tar.gz`；不要覆盖 core 的 source.tar.gz。
上传 AudioBuilder.Dockerfile 与经工具校验的七个 deb/SHA256SUMS 后，运行 tools/nas-start-audio-build.sh。
重建前 tools/nas-reset-stopped-audio-build.sh 核对镜像/标签/挂载，归档后只移除已退出的专用容器；保留缓存。
目前 core build 容器也退出 0，源包 60 文件与 Git 检查点逐字节相符。

音频只读探测产物本机 `.local/artifacts/rustsimhub-audio-alsa-p3`、棒子 `/opt/rustsimhub/dev-audio-p3/rustsimhub-audio-alsa`。
tools/device-audio-probe.sh 以非 root、只放行 controlC0 的临时单元通过 profile/7 项路由读取；探测前后 PCM closed，旧四个服务 active。
一次性单元执行结束并 collect。本轮没拨电话或发短信；前一轮 10010 通话授权已使用，不能重放旧 once 脚本。
完整日志在 `.private/audio-*`；新机器接手时保护这些材料，不能提交或关闭 SSH 主机密钥校验。

下一步：先接受限 core/media 授权 UDS 和持续撤销，再取得新的明确测试目标授权，验证 CS-Voice + 双向 PCM、释放和真实声音。
通过后接 Opus/WebRTC/ICE/TURN；继续 P0 完整恢复、三种网络接入、生产权限、系统镜像及长期验收。
