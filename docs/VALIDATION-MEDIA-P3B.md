# P3B 核心授权、真实 PCM 与退出撤销

2026-10-13。整体工程估算约 38%，采用 PROGRESS-SCALE.md 固定权重。
新增 core/media UDS、真实 Principal / scope / call lease / ACTIVE 快照门禁及受限音频进程。
两秒和五秒真实 PCM 诊断成功，退出撤销成功；上行仅数字静音，没有保存录音，也没有客户端听音。
这不等同于双向可听语音、WebRTC、完整固件或脱离 NAS 的冷启动验收。

## 核心检查点与测试

授权接线检查点 `4831e4ef5a355edba0fe7fed325f58c256e06f9f`。
109 项 workspace 测试与 4 项原生 ALSA 测试通过；
[该检查点 CI](https://github.com/Viper-Boss/RustSimHub/actions/runs/38352630839) 的 fmt、test、默认 core check、Clippy -D warnings 成功。
后续 PCM 调度增加一项抖动回归；后续 HTTPS / 网络状态是另一个进行中的工作包，不能以本记录声称其已实机验收。

core 使用 CLOCK_BOOTTIME；媒体票据有效期 5 秒且一次兑换，每次续验最多 1 秒，每 250ms pulse。
模拟覆盖真实 UDS、错误 UID、非法帧、重放、scope、租约、held、principal 撤销、号码变化、binding 丢失和断开。
不同号码的对象路径复用不会触发旧通话的自动挂断。更复杂的同号码歧义仍未完全消除。

棒子无拨号拒绝测试通过：伪租约不能发行票据；伪票据在 PCM 打开前被拒绝。
该测试媒体单元启用 PrivateDevices=yes，未放行音频节点。两个 socket 最终删除，PCM 全部 closed。

## 实际通话次数及结果

用户先授权一次、随后于 2026-10-13 明确持续允许拨打 10010。后续该目标不再逐次询问。
本阶段实际拨打 **11 次**，每次独立随机状态目录、幂等键、不可覆盖 once 标记，独立 10 秒挂断兜底。
此前通话控制阶段的一次不计入此 11 次。两个准备阶段失败没有提交拨号，不计为通话。

| 本阶段次数 | 结果 |
|---:|---|
| 1 | 脚本在缓存 dialing 状态过早结束，未启动 PCM |
| 2 | ACTIVE；PrivateTmp 隐藏票据目录，媒体在打开 PCM 前拒绝 |
| 3 | ACTIVE 与授权通过；ALSA 配置被拒绝 |
| 4 | 确认 CS-Voice 最小 period=1024，旧候选 40 被 EINVAL 拒绝 |
| 5—8 | holder 修正后采到非零下行；播放欠载超过上限，主动关闭；没有放宽欠载上限 |
| 9 | 两秒诊断成功：capture=92、playback=93、peak=15695、xrun=1，结束 closed |
| 10 | 五秒诊断成功：capture=246、playback=246、peak=15467、xrun=1，结束 closed |
| 11 | 两秒定时停止 core；媒体授权丢失并关闭，capture=94、playback=95、peak=17086、xrun=1；核心自动挂断 |

9—11 的上行峰值均为 0。只是验证静音样本进入播放路径，不能证明远端听到真实麦克风声音。
启动时一次播放 EPIPE 可恢复，之后五秒诊断没有持续欠载；首次启动欠载原因、较长压力及真实端到端延迟仍需验证。
11 因主动撤销以有限拒绝错误退出，是预期拒绝结果。未做活动通话 SIGKILL；core 崩溃后的硬件挂断兜底尚未验收。

十一份已停止的独立私有 SQLite 数据库 integrity_check=ok。
每份均只有一个 dial 和一个 hangup，均 submitted；没有 sms_operations。
最终所有现存 MM Call 对象均已 ended，active_calls=0，所有 PCM closed。
每次临时 core/media/定时单元均停止，旧 nassimhub-agent / ModemManager / NetworkManager / rmtfs 保持 active。
没有发送新短信、启用蜂窝数据、更换系统、写 NV/校准、写 mixer 或访问旧业务数据库。

## 已确认的 ALSA 配置

声卡有限 ID `nassimhubufi003`；只接受已知 PCM ID 或其 ` (*)` 后缀。
七项路由原本均为 on，本阶段只读核验，没有路由初始化写入。

| 流 | 格式 / rate / channels | period / buffer | 行为 |
|---|---|---|---|
| D4p、D4c CS-Voice | S16_LE / 8000 / 1 | 1024 / 2048 | prepare，不读写控制流样本 |
| D1c MultiMedia2 | S16_LE / 16000 / 1 | 320 / 960 | 非阻塞采集，积压有界 |
| D0p MultiMedia1 | S16_LE / 16000 / 1 | 320 / 2560 | 非阻塞播放，160ms 硬件缓冲 |

rate/format/channels/period/buffer 精确回读，不静默接受 near 参数。停止阈值设为 buffer；
播放启动阈值为 buffer-period，采集为 1。20ms 帧调度锚定期限，避免小抖动累积成持续供帧不足。
超过 3 次 xrun/秒关闭；ESTRPIPE 不循环 resume；关闭 drop，禁止 drain 旧语音。
硬件缓冲 160ms 是当前可运行配置，不能据此声称端到端小于 200ms。

## 二进制及证据

11 次核心测试使用上述已提交授权检查点的开发包：
core `060de4cb06ac9832b6f18e266b30ac168b267c91266ce6d2fb47542d65f56aaa`，5395936 字节；
CLI `39b76f9cc63d823927656175745a8bd3def3e27293b5d83033e680c4b3cb7478`，857104 字节。
初版授权源包 SHA-256 `ccd7db92cb237f2114bc838f7c410cc40899f9add86f438d79243f349d7461f4`，64 文件匹配该 Git 检查点。

9—11 的调度修正版 native 产物 SHA-256：
`a42c477f433d16326599cdd3d5fafbfc4763d85c0a902650771f00d7a80c9af1`。
其代码在本轮后续检查点保存；诊断迭代源包、构建日志和二进制保留在 NAS history/audio-build-*，不会用后续源码替换既有证据。
派生构建镜像仍为 `sha256:5a4a245b4ec7387f761b87a7ffefbc2ad2b7ee047ca2297c3b066ff75dedba0c`。
音频过程 UID 为既有 nassimhub，严格放行 controlC0、D0p、D1c、D4p、D4c；媒体目录显式只读绑定至 PrivateTmp 命名空间。

本机 `.private/media-evidence/1..11/` 保存该项目独立库、有限统计和时间；summary.json 为无正文、无号码的操作核验摘要。
其余 `.private/media-carrier-audio-*.log`、`media-audio-*-readback.log` 保留完整私有诊断；禁止提交。
棒子 once 标记 `rustsimhub-carrier-audio-p3b-20261013[或次数后缀].once` 均保留，不删除或复用。

## 待完成

Opus / WebRTC / DTLS 对端绑定 / ICE / TURN、真实上行听音、来电 / DTMF、冷启动路由设置、
活动通话崩溃处理、生产专用用户与开机部署、较长压力和完整系统恢复。
`media_available` 保持 false，只有 `media_authorization_api` 表示授权接线已启用。
