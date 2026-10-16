2026-10-16 最新：整体73/100，10010累计28。一键页面已完成真实自动登录/提交/接音/清理流程，但约12.5秒媒体后Core许可拒绝导致HTTP403；独立保护已挂断，原系统服务/PCM恢复。详见VALIDATION-BROWSER-MEDIA-P3I.md；不加进度，先自动20秒测试查清Core拒绝原因，暂不要求用户再试听。

2026-10-16 当前：整体仍 73/100，10010 累计 26；实机捕获空音频负载触发 Codec 致命退出。新增空负载安全丢弃与一键 10010/自动连接声音/约30秒结束页面源码，202 workspace、21 Native、Clippy 和 Node 三组通过；新 ARM 与真实听音待验收，不加分。见 VALIDATION-BROWSER-MEDIA-P3I.md。

2026-10-16最新：71506f0冻结ARM/Native通过，正式Media可信安装和签名Core/Media/Edge组、实际Media SIGKILL自动整组回退和第二次Edge故障预算耗尽全停在候选根实机通过；P6+0.5，总体73/100。新增一次浏览器10010，累计25，4—5秒后明确Codec错误退出，P3不加分；细分拒绝与乱序/重复包处理正在验证。见VALIDATION-MEDIA-STARTUP-P6K.md。电话/PCM/权限/预览服务已清理；连续诊断的同一临时证书仍需结束后实际移除，当前状态以HANDOFF及私有trust记录为准。流量等换卡，完整系统上电/恢复门禁保持。

2026-10-16交互历史：实际Chrome配对/登录/防重和真实下行一句已发生，但媒体打开约3秒后提前退出，30秒后保护才挂断，持续声音未通过。新增实际10010两次，累计24，中间一轮只等待未拨号。预览/Voice权限/PCM及临时Windows证书已清理；见VALIDATION-BROWSER-MEDIA-P3H.md。新增Media正式启动/可信初装源码和有限退出分类诊断，本机202 workspace/20 Native与Clippy、39板级/Node通过；新ARM与服务组实机待验收。仍72.5/100约73%，不将部分听音或源码当成故障已解决。

2026-10-16：严格HTTPS两阶段媒体在USB/Wi-Fi各一次10010通过，真实DTLS/Opus/PCM到NAS解码159/147帧；重复激活防重、Edge正常退出后Core仍活而PCM关闭、相同身份/证书名/Origin/owner token跨两个真实监听地址通过。四份私有报告passed/cleanup_verified，旧四服务active、电话空闲、PCM关闭、权限撤回。累计10010=22，新SMS/蜂窝/分区写入0；保留可恢复欠载/少量丢帧。874f01c六程序ARM/Native ARM、本机201/20与Clippy/Node通过。见VALIDATION-WEB-MEDIA-P3G.md；P3+0.5、P4+0.5，总体72.5/100约73%。浏览器权限/麦克风/听音、产品服务组硬退出联动及完整系统/恢复仍缺；换卡流量延后。

2026-10-16历史：可信首次安装、持久TLS及设置码/身份的Root私有交付、未初始化不启动和重复安装拒绝在棒子候选用户态实机通过；真实崩溃整组回退回归及清理通过。新组合12338项/树3e3f51ce…、离线启动依赖与树保持核验通过，191项Rust/38板级/六程序ARM/CI通过。见VALIDATION-INSTALLATION-P6J.md。P6加0.5至6，总体70.5/100约71%。尚未实际NSHSTATE初始化、新系统上电或刷写，恢复/维护/媒体仍缺；拨号/SMS/蜂窝新增0。

2026-10-15：统一签名开机组在棒子候选用户态通过实际生产单元验收：完整Core/Edge、首次owner/TLS、跨FS台账、真正Core SIGKILL自动整组回退、第三版显式健康确认/重启及第二次故障停止；清理及归档内容/12303项路径清单通过，旧四服务active/PCMclosed。新签名系统树12335项已审计，187项Rust/35板级/34rootfs通过。见VALIDATION-BOOT-GROUP-P6I.md。P6加1至5.5，整体70/100；未完整上电/刷机，配置/维护交付、恢复及媒体等仍待完成，拨号/SMS/蜂窝新增0。

2026-10-15最新：9b5f1d9本地准备设置码/UID核验公钥与窗口实机通过，185项/六程序ARM生产/Clippy/CI38752335688通过，见VALIDATION-LOCAL-SETUP-P6H.md。SSH持久host key与公钥策略源码、30项板级测试、候选ARM ssh-keygen/sshd -t/-T合成检查通过；未装真实维护公钥或启动新SSH，见persistent-ssh-maintenance.md。完整开机组/可信配对交付/媒体/恢复仍需推进，69/100不额外加点，无新通话/SMS/蜂窝。

2026-10-15补充：a10a492冻结源1dbc6775...本机WSL181项、六程序ARM生产构建、CI38749275454成功；新签名factory首装/候选回退/floor保持及丢失台账拒绝和next恢复在候选RootDirectory通过，见factory-application-baseline.md。不额外计点，仍69/100，正式启动/首次配对和完整恢复待做。

最新（2026-10-15）：候选用户态实机Core/Edge归属、首次设置/真实SIGKILL、短信/Wi-Fi及运行后全树比较通过；同源Native构建和Media984合成Opus/DTLS/SRTP通过，见VALIDATION-CANDIDATE-RUNTIME-P6G.md。P6加0.5至4.5，总体69/100。无新电话/SMS/蜂窝，流量等待换卡。签名factory首装和丢失台账拒绝组件9项测试及clippy通过，尚待冻结构建/实机验证。正式首启/完整启动组/媒体/恢复仍待完成。用户可协助拔插，目前无需操作。下面为历史记录。

最新（2026-10-15）：用户目标100%，实际仍68.5点、约69%。724fc09内部板级候选树组合/通用审计、23项隔离板级测试、离线启动依赖及factory实际路径六程序只读运行通过；真实NSHSTATE守卫接受、空目录/只读别名拒绝，旧身份/挂载owner/四服务/PCM保持。NM配置已接入持久目录，详见VALIDATION-SYSTEM-TREE-P6F.md；不把候选层和重复程序运行额外计点。应用自启组、可信正式首启/维护、媒体及完整恢复仍待完成，不出刷机包，不新增拨号/短信/蜂窝测试，流量等待换卡。此前ca596a7首个owner同进程转业务及崩溃窗口边界实机已通过，见P6E。下面为历史记录。

最新实证：非 Root Wi-Fi 试连接、真实错误口令/无 IP 临时 profile 和核心 SIGKILL 后 NM 自动恢复通过；USB/WWAN 无线保留，临时 profile/权限规则已清理，不明操作不重试。见 VALIDATION-WIFI-TRIAL-P4A.md。整体约 56%，继续至 75%，蜂窝流量最后测。

最新实证：Core 986 和 Media 984 的独立非 Root 电话加密音频通过，空能力集合；实际最小 Voice 权限与其他控制拒绝、规则移除后恢复拒绝通过。累计 15 次 10010 拨号。见 VALIDATION-ISOLATED-MEDIA-P7A.md。整体约 56%，75% 长任务继续。

最新实证：签名应用槽位在棒子验证通过，生产 core 的实际候选进程 SIGKILL 后选择器回到 factory，身份和短信内容一致，低版本/换钥/签名及 payload 篡改拒绝。见 VALIDATION-APP-UPDATE-P6A.md。整体约 56%，继续推进 75%；普通蜂窝流量最后验收。

最新实证：10010 真实电话下行经 Opus + DTLS/SRTP 到 NAS 解码成功，150 帧；静音上行、自动挂断、PCM 全关闭、旧四服务 active。见 VALIDATION-RTC-CARRIER-P3E.md。整体约 56%，75% 长任务继续，流量最后测。

# 开发进度

2026-10-16源码推进：网页两阶段媒体适配、受信任连接地址/owner绑定、单次票据内部化、Native接管清理与多bind白名单、Edge多私有监听及浏览器麦克风/播放/取消实现。本机201 workspace/20 Native/Clippy及Node媒体生命周期和proof通过；ARM/HTTP实机/真实浏览器仍待验证，不加进度点，仍71.5/100约72%。见web-media.md。没有新增电话、短信、蜂窝或分区写入。

2026-10-16最新：e271ca1/621994eb…的生产Core与常驻Media、受限IPC在候选根中两轮真实10010授权媒体集成通过：123/214帧捕获、NAS123/212帧解码，主动停止/挂断和全部隔离清理通过；分别1/2次欠载成功恢复，不能称长期稳定。194 workspace/19 Native/Clippy及同源六程序ARM与Native ARM通过。P3加0.5至13.5，总体71.5/100约72%，见VALIDATION-MEDIA-WORKER-P3F.md。累计10010=20，短信/普通蜂窝新增0；网页信令、真实双向听音、正式媒体服务与完整刷写仍待完成。

2026-10-16新增：实际9008进入、主备GPT和完整3909091328字节主区读取、一次自动返回及旧身份/服务/PCM保持通过，见VALIDATION-EDL-READ-P0B.md。原始捕获文件系统检查退出4，原图保留；第三份独立副本journal+安全元数据preen后两分区只读检查0、GPT/boot及原图摘要保持。P0加0.5，总体71/100约71%。未完整回刷、未解除内核来源或刷写门禁。

常驻媒体：233110a、173b21f同源ARM构建及真实负向DTLS/权限预检通过。一次10010实际生产Core/票据/PCM与NAS解码已成功到达，但36帧后连续4次欠载退出，未算稳定媒体通过；通话/PCM/临时权限清理通过，累计10010为16。7077565已修正PCM故障误报、降低实机轮询负载，本机194 workspace/19 Native及Clippy通过，六程序ARM构建通过、Native ARM构建进行中；待实际复测，不提前加P3分。流量测试仍等换卡。

2026-10-15：182a6ec完整Core组合与cd4c96e无旧NAS数据初始化两轮实机passed/cleanup_verified=true；短信/历史/NAS在配网期间可读，拨号准备拒绝，模拟真实电话端证明旧prepared意图未消费且挂断可执行；真实SIGKILL后原Wi-Fi恢复、业务去重/NAS映射保留，明确保存后自动重连通过。空历史首次安装不需要NAS原库/示例快照或NAS principal。见VALIDATION-PLATFORM-P6C.md，集成不重复计点，仍66.5点约67%。未刷机/开机安装/物理断电，未新拨号/短信或蜂窝测试；目标80%继续。

2026-10-14：新有限Wi-Fi配置的实际独立profile连接、owner确认持久保存、Core退出后NM自动重连、错误PSK候选存在时真实SIGKILL恢复及跨重启防重通过；本轮连接既有SSID，不声明不同SSID/热点/未配置首启或完整Core组合已验收。见VALIDATION-WIFI-PROVISION-P4B.md；P4加1至7/12，总体66.5点、约67%。170项本机测试/lint、六个ARM64生产构建及CI38570475237通过。第二轮私有报告passed/cleanup_verified=true，旧profile/WWAN保留、自有配置/规则/服务已清理，旧四服务active、PCMclosed，无新通话/短信/蜂窝测试。继续目标80%。

2026-10-14：d20b9af实现有限NAS桥，530e6f5增加实际观察只读模式与联合Go验收；两轮私有报告passed/cleanup_verified=true。P5加1.5点至5/8，总体65.5点、约66%。源码/源归档/产物核验、本机163项测试、六个ARM64生产release及CI38565794201通过。530e6f5本次增量release34.24秒，不作为全量构建/等功能性能对照。自有单元停止，旧四服务active、PCMclosed，无新拨号/短信/蜂窝测试。下一步NAS页面/媒体整合、网络与完整系统剩余工作；目标80%继续。

2026-10-14：真实旧NAS通信库离线迁移通过：只读一致快照、重复导入无新增、独立逐字段/摘要一致、仅非认证历史归档到棒子并通过非Root只读检查。见VALIDATION-HISTORY-P5C.md。增加P5半点，整体64点、约64%；真实在线查询与NAS协议/PCM仍未完成，目标80%继续，蜂窝测试跳过。

2026-10-14：正式历史查询已接入组合Core/HTTPS，157项本机测试、ARM生产、CI及实际跨用户/来源/标记/tombstone/撤销与generation/重启/无业务重放通过。见VALIDATION-HISTORY-P5B.md；整体63.5点、约64%。真实旧库与NAS在线协议/PCM、浏览器显示仍待验收；流量继续跳过。

2026-10-14 本机交叉编译验收：稳定8f4d15b 的151项Linux测试、六个生产ARM64程序及NAS Debian13容器/棒子隔离非Root无硬件运行自检通过。默认本机WSL 8路编译，持久缓存；原生音频仍NAS构建。见VALIDATION-LOCAL-CROSS-P7C.md。此为构建流程优化，整体仍62.5点、约63%；历史查询WIP尚未接线/提交/验收。

最新约束（2026-10-14）：当前卡没有流量，普通蜂窝数据实测最后进行。只使用 USB/Wi-Fi 做网络/媒体验收；禁止普通 APN 激活、互联网蜂窝探测或隐式流量回退。只读状态与预算模拟可继续；不关闭语音使用的 WWAN/IMS。10010 持续授权仍有效。


新增 VALIDATION-RTC-P3D.md：棒子真实 UDP/ICE/DTLS/SRTP 合成回环、NAS 与棒子五秒双向编码静音通过。没有打开 PCM、拨号或新增短信。
native aff5dcd 已部署 /opt/rustsimhub/dev-rtc-p3/，self-check 双向 50 帧；LAN 各发 250、收 250/249，解码静音只有 1 LSB 量化残差。
最新源码补充证书替换失败、信令输出原子发布和一秒媒体断流关闭，尚须下一次原生构建验证。root 2c69ff9 的 125 项、默认 production core 构建通过，prod/development 分开；P6 实机签名应用验收仍未加点。
不要重跑旧电话 once；用户持续允许 10010，但新的电话测试要用独立标记且确保挂断。约 75% 长任务继续。


更新时间：2026-10-14 / 实际浏览器与 Wi-Fi 只读业务；整体约 **56%**（按 PROGRESS-SCALE.md 固定里程碑估算）。

实际浏览器经回环 QA 代理严格验证设备 Wi-Fi HTTPS，配对/批准/签名登录、短信正文与摘要、事件分页、只读发送准备拒绝通过。
复用隔离身份/授权重启后旧 token 拒绝，新登录通过；退出清空显示。
修正并实际验证在途只读响应延迟时能立即退出，旧响应不恢复界面；最新静态 release 已通过不同 UID 实机烟测。
Opus 固定 16k/20ms 适配器独立原生构建通过；棒子 500 帧合成往返成功，未打开 PCM，见 VALIDATION-CODEC-P3C.md。
见 VALIDATION-BROWSER-P2.md。约 75% 长任务仍活跃。以下为历史记录。

新增 Rust HTTPS 入口与静态网页，NetworkStatus 前后重新鉴权并核验唯一 NM owner；120 项 workspace、CI Clippy、ARM64 release 通过。
不同 core/edge UID 实机验证：严格 TLS、离线 owner 签名、只读 SIM/短信摘要/网络、scope 与撤销、错误 UID 和 Origin/role 注入拒绝。
外部 NAS 经 USB HTTPS、公钥/证书核对通过；自有临时单元已停止，旧四服务 active，PCM closed，无新短信/拨号/网络写入。
修正 Cargo 源包 mtime 造成旧 release 复用的问题；每个二进制自报被编译器跟踪的源包 SHA，部署再核对传输 hash。
只读配置使用 O_RDONLY 与实际 fd 的 UID/mode/nlink 检查；不通过可写挂载绕过安全边界。
网页浏览器交互、网页写业务、实际 Wi-Fi 入口/配网、蜂窝预算、WebRTC 与完整系统仍在后续工作中。
见 edge-and-network.md / VALIDATION-EDGE-P1.md。约 75% 的长任务仍活跃，不在本小检查点停止。以下为历史阶段。

core/media 授权 IPC 已接通；真实 Principal、scope、独占租约、ACTIVE、SIM/backend/path fence 在每次续验重新验证。
实机两秒和五秒 PCM 运行成功，五秒采集/播放各 246 帧，上行仅静音；启动一次 EPIPE 可恢复，未放宽欠载上限。
核心正常退出时实际授权丢失、PCM 关闭、自动挂断通过。11 次独立 10010 测试私有库均一个 dial / 一个 hangup，无新短信操作。
用户已持续授权本项目拨打 10010，无需再次逐次确认；其他号码或新短信不推定授权。
验证和产物见 VALIDATION-MEDIA-P3B.md / media-authorization.md。media_available 仍 false，WebRTC / 真实双向听音未完成。
HTTPS 调试入口、WebCrypto 客户端、网页和网络只读状态正在开发验证，未作为已完成硬件验收计分。
用户要求持续长任务推进至约 75%；该目标仍在进行，不能因本检查点保存而停止。下面为此前阶段记录。

新增 crates/media：固定 16kHz/20ms PCM、有界帧队列、分段 I/O、静音/断流/xrun 处理和媒体票据/撤销原语。
新增独立 native/audio-alsa：有限 UFI003 profile、两个 CS-Voice holder 及采集/播放句柄，safe Rust 调用 libasound。
102 项 workspace + 4 项原生 ALSA 测试通过，源码 CI Clippy 成功，NAS 两个 ARM64 release 构建成功。
非 root 棒子只读探测通过：profile 匹配、7/7 路由开启，PCM 未打开、mixer 未写入；旧服务正常。
core/media IPC 与真实授权还没接通，media_available 仍为 false；真实双向声音未验收。
已将用户要求的“每次结束报告整体百分比”写入 AGENTS.md，估算口径见 PROGRESS-SCALE.md。
实现与证据见 audio-foundation.md / VALIDATION-AUDIO-P3A.md。下面为此前通话阶段记录。

新增呼叫观察/显式控制模式、v5 持久记录与单次许可、独占租约、过期/撤销/退出清理及私有 CLI。
90 项测试、CI Clippy、NAS ARM64 默认检查/release 通过。
非 root 实机只读检查通过；独立 root 写模式仅拨打 10010 一次，观察 ACTIVE 后自动挂断，最终 state=7、active_calls=0。
私有 DB 核验只有一个 dial 和一个 hangup，均 Submitted/resolved，没有新 SMS 操作。临时服务已停止，旧四个服务及网络正常。
双向声音、真实来电/DTMF、生产非 root 写权限及完整固件尚未验收。
详见 call-control.md / VALIDATION-CALLS-P3.md；下面为此前短信阶段记录。

本轮新增显式 --serve-messaging、sms_send 操作权限、v4 持久单次许可和 CLI 私有草稿。
79 项测试、CI（含 Clippy）、NAS ARM64 默认检查/release 通过。
非 root 只读实机回归通过；独立 root 写模式只向 10010 发送一次 CXYE，已核验 SENT 并收到私有运营商回复。
一个持久操作及一次提交事件经私有备份只读核验。临时服务已停止，旧服务和网络正常。
实现见 sms-send.md；验证与当前产物见 VALIDATION-SMS-SEND-P2.md。
下面 73 项及此前收件箱结果为历史阶段记录。

本轮完成唯一 MM owner 信号订阅、连接重试、有界批次/逐对象隔离、路径 incarnation、执行前 SQLite 门禁和受鉴权保护的观察计数。
73 项测试及 CI Clippy 全部通过，私有 D-Bus 模拟覆盖坏对象、路径复用、SIM 信号和连接关闭。
实机订阅、已有 1 条短信读取、周期修复和撤销拒绝通过；测试单元已停止，旧服务/网络正常。空闲 RSS 样本约 6.27 MiB。
实现见 [observer-events.md](observer-events.md)，证据见 [VALIDATION-OBSERVER-P2.md](VALIDATION-OBSERVER-P2.md)。旧阶段记录保留。

## 已完成

- 新建本地 RustSimHub 目录、独立 Git 仓库及 Viper-Boss/RustSimHub 私有 GitHub 仓库。
- 详细开发计划已迁入 docs/development-plan.md；新增来源/许可证和接手约定。
- 通过 NAS 的 USB 链路及既有维护密钥连接棒子，完成只读硬件与服务基线。
- 新增 domain/wire/core Rust workspace：通话租约、SIM epoch、短信提交规则、有界协议、自检、Linux 只读 MM 探测及 UDS 服务。
- NAS 的 ARM64 官方 Rust 容器已通过测试和 release 构建；源码以允许列表打包，隔离使用数据盘，不挂载旧应用数据或主机 Docker socket。
- boot 与模块清单哈希已固定；64 MiB boot 备份已保存到本机与 NAS 的私有目录，哈希校验一致。
- ARM64 程序已在棒子运行并完成非 root D-Bus 及 UDS 烟测，拒绝非授权 UID；测试单元已停止。
- 新增 storage crate：单写者文件锁、32 容量请求队列、WAL/FULL、schema v1、持久幂等键、事务事件、SIM 隔离、Unknown 恢复、Backup API 与安全恢复标记。
- 默认 core 构建不提供存储 fixture；显式 dev-fixtures 开发构建在棒子独立目录完成合成数据验证。
- 新增 auth/policy crate、schema v2 身份/权限记录与 v1 迁移；签名挑战绑定双方公钥、purpose、scope、generation 与进程随机 instance。
- 新增离线 owner CLI 和 v2 鉴权 UDS；token 只保存摘要，重启失效，撤销持久生效；此前仅开放只读，当前增加显式短信写模式。
- 新增可信离线恢复、身份明文私有备份、独立客户端密钥/固定设备 pin/私有会话文件与 owner 管理 CLI。
- 鉴权 daemon 支持 SIGTERM/SIGINT 排空、唯一 writer 关闭、运行锁、同路径重启和受限 stale socket 接管。
- 增加显式只读观察 daemon、SIM 身份稳定门禁、原子换卡/outbox 隔离、完整入站短信和持久收件事件、32 条摘要分页及正文单独读取。
- 不同对象的同内容短信保留并标注疑似重复；同对象同内容重复读取返回原记录，换卡保留旧历史。
- 本轮增加独立持续监听任务、owner/连接失效代次、有界目录/路径提示与公平批次；同路径删除/复用或完整内容变化保留新记录。
- 单写者在状态/正文写入执行前检查观察 gate，监听器关闭或代次失效的排队任务拒绝。

## 验证状态

- Windows 格式化执行成功。Windows 原生编译未通过：MSVC link.exe 未安装。
- P0/P1 初始 ARM64 Linux：12 项测试通过（core 2、domain 7、wire 3）。覆盖断开/截断帧、超大帧、角色注入、租约过期与换卡、未知短信禁止重发。
- GitHub CI 已通过：Linux 测试、格式检查、Clippy（-D warnings）。首次运行：[38153648494](https://github.com/Viper-Boss/RustSimHub/actions/runs/38153648494)。
- P0/P1 初始 ARM64 release、自检、真实 D-Bus 与受限 UDS 实机验证均通过。该阶段程序约 1.88 MiB，只读服务 RSS 样本约 3.3 MiB。
- 完整证据与哈希：[VALIDATION-P0-P1.md](VALIDATION-P0-P1.md)。这些结果不能替代完整业务/性能验收。
- 身份阶段 47 项测试及实机记录见 VALIDATION-AUTH-P1.md；恢复/CLI 阶段 59 项通过（auth 19、core 8、ctl 1、domain 7、policy 1、storage 19、wire 4）。CI：[38265061208](https://github.com/Viper-Boss/RustSimHub/actions/runs/38265061208)。
- 恢复/CLI 阶段开发 core 约 4.15 MiB、simhubctl 约 0.75 MiB，完成真实离线 CLI 恢复、重新批准/撤销、SIGTERM/SIGKILL 重启；仍不等同于完整性能或固件验收。
- 板上旧服务仍承担日常业务；本轮新代码完成一次授权测试短信，本轮另完成一次授权拨号/挂断，未启用蜂窝数据或改写分区。
- 收件箱阶段源码 fdaf4c2：66 项通过（auth 21、core 10、ctl 1、domain 7、policy 1、storage 22、wire 4），CI：[38298747817](https://github.com/Viper-Boss/RustSimHub/actions/runs/38298747817)。NAS 默认 workspace 检查/release、自检及实机已有短信读取通过；该阶段 core 约 4.40 MiB，CLI 约 0.75 MiB。
- 事件观察阶段源码 31be30a：73 项（auth 21、core 16、ctl 1、domain 7、policy 1、storage 23、wire 4），CI [38316851912](https://github.com/Viper-Boss/RustSimHub/actions/runs/38316851912) 成功；NAS 默认检查/release、实机订阅与周期修复通过。该阶段 core 约 4.52 MiB，CLI 约 0.75 MiB。

## 阶段状态

P0 硬件/恢复基线进行中：系统和权限已检查，备份及恢复入口未闭环。
P1 基础工程进行中：纯规则、协议、持久化、公钥鉴权、恢复/客户端 CLI 与关闭重启已实现并验证；仍缺网页密码/TLS、最终用户设置 UI、完整开机部署和系统恢复。
P2 进行中：SIM 观察、短信接收/读取、订阅/重连、基础 Create/Send 与 Unknown 对账已实现并验证；历史归属/歧义、保留策略、长短信/投递报告、生产权限与更多硬件验收未完成。
P3 进行中：基本通话控制、独占租约和一次拨号/挂断实机通过；PCM/媒体授权原语与原生 ALSA 基础已验证，核心授权 IPC、真实声音、WebRTC、真实来电/DTMF与跨重启通话管理待验收。
P4–P7 未开始。当前不是可刷写完整固件。

## 下一步

1. 从实际呼叫控制器生成可信媒体授权快照，接通受限 core/media UDS、单次票据与持续失效通知；再做 CS-Voice/PCM 实机打开和双向声音验收，随后连接 WebRTC。
2. 补齐真实来电/拒接/DTMF、跨重启孤儿管理、历史状态/路径歧义、生产非 root 写权限及 systemd/TLS。
3. 短信历史/外部事件、保留策略、长短信/投递报告与编码/存储满硬件验收继续；Unknown 禁止自动重发。
4. 本轮一次电话及此前一次短信授权都已使用；不能删除 marker 或重放实机测试脚本。
5. 继续 P0 精确内核来源、完整私有备份/恢复入口，随后网络切换、加密远端接入、OTA 和系统镜像；完成恢复闭环前不刷 rootfs/boot。
