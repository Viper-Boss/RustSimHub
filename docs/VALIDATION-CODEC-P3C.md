# 固定 Opus 编解码验收

2026-10-14。增加 P3 一个进展点，整体约 48%；WebRTC 和真实双向语音继续开发。

源码 e9d6e2f，允许列表源包 SHA256 `65f5b25d9899edf2bd0df32a4b660985476af6cc870d6f15f374cd3c770a840a`，74 文件与该 Git revision 逐字节一致。后续工具支持指定不可变 archive/revision；不要把旧产物归到新的 HEAD。

Workspace 120 项、原生 7 项通过。原生 3 项 codec 测试验证实际 Opus 往返、包长/时长、陈旧/未来/重复/撤销拒绝。16kHz 单声道 20ms、24kbps CBR、复杂度 3、DTX 关闭；RTP 时钟 48k/步长 960。编码/解码前后都检查同一媒体 Permit。仅可信解密传输可提交收包时间和 extended sequence。

Opus 1.6.1 来自锁定 opusic-sys，静态链接，DRED/OSCE 关闭，hardening/stack protector/fortify 开启。29 个校验 SHA256 的离线编译依赖只进入隔离 NAS 镜像；宿主和棒子不安装开发包。完整 BSD 许可证见 licenses/Opus-COPYING.txt，已与棒子原生产物一起部署。

ARM64 native 二进制 1,053,592 字节，SHA256 `8dfb5fc87fda5d61dab79f795e9e57aae47a29d1a92cf40c83dfafb14ae4ecf6`。自报源包 hash 匹配；动态依赖没有 libopus。棒子 `/opt/rustsimhub/dev-codec-p3/`，非 root、PrivateDevices 的合成测试：500 帧、30,000 编码字节、解码峰值 3108、耗时 948,978 微秒；systemd 运行 1.116s，CPU 1.050s，内存峰值 1.8M。没有打开 PCM、录音、拨号、短信或网络写入；旧四服务 active。

单次合成样本约 1.90ms/帧编解码，不能代表完整媒体负载或实际通话质量。秘密帧不实现 Debug/Serialize，丢弃时清空缓冲；C 库内部清零和编译器保留清零尚未证明。丢包补偿、重采样、真实网络与 ALSA 接线待验收。

最终网页取消修复同源包完成 ARM64 release 和不同 UID 严格 TLS/Core 鉴权烟测；没有新增硬件写操作。私有证据：`.private/codec-device-synthetic-check.log`、codec-native-metadata.log、browser-cancellation-release-smoke.log、browser-codec-core-metadata.log。原始日志不提交。
