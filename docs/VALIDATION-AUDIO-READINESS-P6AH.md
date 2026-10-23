# 开机音频设备状态的有界等待

2026-10-24。第二次普通拔插已验证首次直接启动成功；整体79.5%，修复不重复增加已有冷启动分数。下方保留实现、首次失败及修正过程。

## 现场问题

P6AG保存Wi-Fi的冷启动最终能自动恢复业务，但完整日志显示首次音频初始化报PCM status unavailable，媒体依赖失败、Edge预检查退出，随后消耗本次启动的一次整组恢复预算。模块加载完成并不保证声音设备已经注册；旧程序把固定PCM状态文件未出现立即作为失败。日志没有保留具体errno，不能断言所有类似故障都来自同一种原因。

关机/PMIC/SMD调查继续搁置。本项只处理用户态启动准备，不重启基带、不改驱动、不修改或删除recovery.used，不延长媒体租约，不放松设备权限。

## 修改及边界

`native/audio-alsa/src/routes.rs`保留Root及固定板型检查，随后最多等待20秒，使用单调时钟，每100毫秒读取原来的四个固定PCM状态文件。仅NotFound可以重试；每轮检查全部四项，早先某项缺失不能遮住另一项正在使用。全部明确为closed才进入原有严格profile检查和七路初始化。缺失从不视作closed。

占用、无效内容、权限错误、其他读取错误立即失败；读取耗时和调度晚醒不重置截止时间。只有初始化命令有该等待，普通媒体许可/打开PCM路径没有增加重试。成功时日志输出platform_readiness ready_ms，便于区分初始化等待和通话接音延迟。

对应音频路由oneshot的TimeoutStartSec由10改30秒，为20秒枚举等待与后续控制检查留出空间。仍只允许controlC0，不打开PCM；启动失败后的既有单次整组恢复机制保持。

## 已验证

- 本机WSL音频33项测试及严格Clippy通过；新增7项覆盖立即就绪、四路逐步出现、固定超时、等待中变忙、早先缺失但后续占用/内容异常、非NotFound错误，以及慢读/晚醒。
- 本机WSL Root运行系统夹具154项通过，1项按条件跳过；普通用户运行时84项因Root条件跳过，不能冒充同等覆盖。
- 固定88692b4同源ARM Native33项及release、完整CI39985182741通过；独立Root组件及30秒单元已备份交付，业务仍935ea9d/confirmed31。新冷启动首次直接成功的验收仍未通过。

真实冷启动是否不再触发整组恢复，需要更新交付后单独验收；当前已使用的启动预算仍保留，不能通过当前热运行的成功替代新一次冷启动。

私有检查日志：audio-readiness-check-20261024-first.log、audio-readiness-system-root-20261024-first.log。原P3L来电验收及67条短信基线保留。

## 首次冷启动失败与设备许可顺序修正

首轮NAS容器1GB限制导致OOMKilled，记录保留。第二轮单路编译/1.5GB通过，两个已退出容器均清理，源码、日志、产物及缓存保留。固定88692b4 Native的SHA-256为15819aad8368098c595d4d04a95761964d38e270c6b46390c3f33e04c71ee29e。安装前在与原单元相同的control-only/空capabilities限制下执行，changed=0、不开PCM。替换有本机及棒子两份旧文件备份，业务PID、epoch31、Owner/67短信/所有旧操作及恢复标记保持。

用户普通拔插后的新boot，18.39秒启动初始化，21.15秒四项PCM状态出现，21.17秒ALSA声卡名解析失败Control(19)。控制设备的udev处理随后才出现；35秒既有整组恢复再启动初始化，七路0→7成功。首次检查严格拒绝，原报告保留；独立第二份只验证“恢复后数据与业务健康”，原67短信/Owner/身份/TLS/confirmed31及普通数据关闭通过，不能替代首次直接成功。

进一步发现与现场时序一致的设备访问许可机制：服务启动早于controlC0节点，DeviceAllow无法在该时刻解析它；进程稍后等待到PCM状态后仍无访问许可。设备单位dev-snd-controlC0.device已在目标系统确认active/plugged，标准udev规则存在。修正为同时Requires/After该精确设备单位，保留仅controlC0的DeviceAllow；整体排队任务60秒上限，进程启动仍30秒，Rust等待仍20秒，不扩大到所有ALSA设备。

新增真实systemd夹具使用未分配major/minor的合成字符节点：先创建限制组、后出现节点，打开返回EPERM；将节点生成作为有序依赖后，返回无驱动的ENXIO，证明许可已建立，未打开实际硬件。保留生产单元的空capabilities/closed设备策略。夹具清理所有临时单位和节点。后续须交付该单元并再做普通冷启动；不修改Rust或重编业务来掩盖顺序问题。

参考：[systemd设备许可](https://www.freedesktop.org/software/systemd/man/257/systemd.resource-control.html)说明许可在服务启动时解析；[任务超时](https://www.freedesktop.org/software/systemd/man/257/systemd.unit.html)区分排队任务与服务启动超时。目标首个失败进程已退出，未直接读取其过滤器；虚拟设备确定性复现证明该机制存在，目标因果归属仍须后续冷启动验证。

私有入口：audio-readiness-native-20261024-second.json、audio-readiness-delivery-20261024-first.json、audio-readiness-coldboot-backup-20261024-first.json、audio-readiness-coldboot-health-20261024-first.json（首次失败）、audio-readiness-coldboot-health-20261024-second.json（仅恢复后健康）、audio-control-device-order-20261024-first.json、audio-control-order-fixture-20261024-first.log。

## 有序设备依赖的交付和第二次普通冷启动：通过

固定0a823e2只更新初始化单元：Requires/After精确controlC0设备、JobTimeout60秒，原control-only/空capabilities和Rust20秒等待保持。Native仍为已验证88692b4；正式业务仍935ea9d/confirmed31。155项Root系统夹具（1跳过）及完整[CI39989478746](https://github.com/Viper-Boss/RustSimHub/actions/runs/39989478746)通过。旧单元在棒子和本机两份备份，目标systemd解析的依赖、30秒进程超时和1分钟排队超时均读回；业务PID/账本/恢复标记保持，未重启进程来假装冷启动。

首次冷启动后NAS USB维护IPv4未自动激活，第二次备份尝试因USB SSH连接失败在停止业务之前结束，记录保留；新的第三份备份改用相同SSH身份钉住的既有Wi-Fi通道，一致性归档276480字节、独立回读及数据库检查通过，然后暂停业务并同步。用户按明确步骤普通拔插，没有刷写、调用关机驱动或重置恢复预算。

第二次新boot的单调日志：音频驱动19.33秒完成，精确控制设备就绪后23.73秒开始初始化，23.86秒ready_ms=0、七路changed=7且不开PCM，24.44秒Core、24.65秒Media、25.07秒HTTPS Edge启动。仅一次路由初始化；app-recover日志为空、新boot不存在recovery.used。保存Wi-Fi连接、NAS USB维护IPv4尚未激活时严格HTTPS三项静态文件精确匹配935ea9d。

同一身份、唯一Owner、原67条短信、原操作收据/Unknown、删除标记、TLS、私有profile和PS语音偏好、三个签名业务程序及confirmed/floor31均核实保持。本轮没有新增短信候选、拨号、短信发送或普通蜂窝数据。结束时PCM关闭。此次只验证启动和数据/设备就绪；双向听音是此前P3L用户验收，新boot未再拨号。一个成功样本不代替长期、多次启动或第二台设备。

首份严格失败、独立恢复后健康、第二次真实首次成功三种结果分别留存，不篡改失败，不重复加分，整体79.5%。本轮无待用户物理操作，临时浏览器证书仍移除。

最新私有入口：audio-control-unit-delivery-20261024-first.json、audio-readiness-coldboot-backup-20261024-third.json、audio-readiness-coldboot-health-20261024-third.json、audio-readiness-checkpoint-20261024-first.json。
