# P3A PCM 与媒体授权基础

2026-10-13。本阶段是可测试组件，core 的 `media_available` 仍为 false。
没有向现有 daemon 添加媒体写接口，未声称真实通话已经传送声音。

## 实现边界

`crates/media` 是 workspace 的第八个 crate，只有有限 PCM、缓冲及授权原语。
`native/audio-alsa` 是单独的 Linux 原生 crate，拥有自己的 workspace 和 Cargo.lock；
依赖 libasound 开发包，只由专用 ARM64 构建环境检查。它没有网络监听、录音文件或任意 ALSA 名称接口。
当前二进制仅有 `--probe-readonly`，读取控制卡、PCM 元数据和七项路由，不能打开音频流。

核心默认构建不链接 ALSA。这样硬件库和句柄集中在后续媒体进程，
核心不因阻塞音频 I/O 停止处理短信、挂断或撤销。
workspace CI 的测试与 Clippy 不覆盖被 exclude 的原生 crate；原生部分用 NAS 独立测试/release，不能将两者混为一谈。

## UFI003 固定 profile

沿用已核对 AirSimHub 源码的有限板级约定，来源见 NOTICE.md。
卡 ID `nassimhubufi003`；媒体下行采集 `hw:nassimhubufi003,1`，上行播放 `hw:nassimhubufi003,0`。
必须同时保持 `hw:nassimhubufi003,4` 的 playback/capture 两个 CS-Voice 控制句柄，仅 prepare，不传送样本。
控制流为 S16_LE / 8 kHz / mono，候选 period=40、buffer=160；媒体为 S16_LE / 16 kHz / mono，period=320、buffer=960。
参数必须逐项读回一致；不采用 plug 转换、不回退到麦克风、不隐藏 unsupported。
控制流的具体 period、媒体 buffer 以及同时打开顺序尚需真实通话验证；旧 C holder 只证明 set_params 的 20ms 延迟要求，不能证明本次精确参数已可用。

七项布尔路由：Voice Session VoLTE、PRI_MI2S_RX Voice Mixer CS-Voice、CS-Voice Capture Mixer TERT_MI2S_TX、
MultiMedia2 Mixer VOICE_RECORD_RX、VOICE_PLAYBACK_TX Audio Mixer MultiMedia1、Voice Record Downlink、Voice Playback Uplink。
本阶段只验证这些开关已开启；不会写 mixer。独立系统最终仍需要平台层负责冷启动路由初始化。

`UfiDuplex::open` 只接收组件 Permit 与可信单调时钟回调。
每个句柄打开前检查 Permit；发生错误时释放已打开句柄。I/O 为 nonblocking；EAGAIN 等下轮，EPIPE 有限 prepare，
ESTRPIPE/其他错误关闭，不运行恢复循环。不调用 drain，不调用旧 Go/C holder 或 arecord/aplay。
所有本仓库 Rust 源码 forbid unsafe；上游 alsa/alsa-sys 和 libasound 内部仍有 FFI，需要后续审查与系统隔离。
参考 [alsa PCM 接口](https://docs.rs/alsa/latest/alsa/pcm/struct.PCM.html) 与
[HwParams](https://docs.rs/alsa/latest/alsa/pcm/struct.HwParams.html)。

## 帧、队列和生命周期

固定 20ms、320 样本、640 字节。传输边界显式 little-endian，帧不实现 Debug。
上下行各四帧固定队列，满时丢最旧帧；60ms 或更老、未来时间、重复/倒退序号拒绝。
设备分段 read/write 组合一帧；超时半帧丢弃。上行无数据发送静音，调度停顿后不突发补发旧静音。
采集硬件 backlog 超过两帧按 discontinuity 重置；xrun 清除部分帧与两个队列。
一秒内最多三次恢复，第四次关闭。时钟倒退、Permit 失效、撤销、Drop 都关闭设备并清空队列。
每个 step 至多一次 read 与一次 write；后续由独立媒体线程调用，不能放进 core Tokio 控制线程。
统计只有帧数、丢帧、xrun 与峰值，未实现 RMS/延迟分布；没有录音或 PCM 日志。
这不是编解码器、抖动缓冲、重采样器，也不证明 AMR-WB/EVS 已启用。

## 授权原语及尚未接通的接口

Authority 只能接收 **可信核心快照** Context，包含 boot、line、call incarnation、独占租约 generation、
principal ID/generation、接收 peer key、实际 ACTIVE、MediaUse/CallsControl 判定、核心保存的租约截止时间。
Context 的公开 Rust 字段供受信组件集成；绝不能直接用客户端 JSON 构造它。
256 位随机 Ticket 仅保存域分离 SHA-256 摘要，最长 5 秒、单次消费、绑定 peer；失败消费也作废。
同一 Authority 只容许一个 pending Ticket 或运行 session；运行会话必须先 revoke 才可替换。
Permit 每次 I/O 检查 live 和截止时间。核心快照需在 1 秒内刷新，否则 fail closed；刷新不能复活已过期 Permit。
core 消失、换卡/换呼叫/换租约/撤销/权限变更时 revoke。

这些原语目前只在组件测试中调用，**没有** auth/wire/CLI 的 ticket 发放、跨进程 ticket 验证、身份可信 peer 检查或持续 core 订阅。
Ticket/Context 未提供网络序列化，不能将本组件测试算成整个鉴权链路已完成。
下一轮在 call controller 内从实际 Principal、CallLeases、Binding 和 ACTIVE 生成快照；
再实现受 UID 限制的媒体 UDS、单次票据与持续失效通知，才可启动真实 PCM。
跨进程时统一 boot-monotonic 时基，不能比较两个进程各自 Instant 的起点。

## 构建

```powershell
cargo fmt --all -- --check
cargo fmt --manifest-path native/audio-alsa/Cargo.toml -- --check
python tools/download-audio-builder-deps.py
python tools/package-source.py
```

将源包单独上传为 `audio-upload.tar.gz`（不覆盖 core 的 source.tar.gz），上传 AudioBuilder.Dockerfile 与 `.local/audio-image-context` 中七个 deb 和 SHA256SUMS，
在 NAS 执行 tools/nas-start-audio-build.sh。只安装在派生镜像内，不修改 NAS/棒子系统包。
Debian bookworm arm64 包版本、HTTPS 来源、SHA-256 固定在 tools/audio-builder-deps.json；下载时与镜像构建时各校验一次。
镜像 build network=none，基底固定 digest，实际镜像 ID 单独保存；包来源以 Debian HTTPS 元数据为依据，未额外验证 InRelease 签名。
容器 UID/GID 1000、2 CPU、1 GiB、pids=256；源码只读，独立 target/Cargo 缓存，没有设备、host network 或 Docker socket。
成功/失败都先保存日志；tools/nas-reset-stopped-audio-build.sh 只移除核对标签、挂载、镜像的已退出专用容器。
源包纳入 native/audio-alsa 的清单、锁文件和 Rust 源，排除任何 target；详情看本轮验证记录。
板级 PCM 匹配使用 snd_ctl_pcm_info 的 ID，不能用此驱动的空显示名称；只接受约定标识或其 ` (*)` 后缀。
只读失败只返回固定阶段/errno，不输出原始硬件信息或语音。
