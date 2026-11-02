# D26补验：编码后迟到帧的实际RTC拒绝

2026-11-02，继续本人PR #170，起点 `d9f1cdeafc8faaf59f014f5438e915c6986af6c6`。新增显式离线复跑脚本，完整受测源码 `99c186f0d0a3f77474f20ffe31e596bf04d66f96`。这次没有修改生产Rust、Cargo/lock、已有测试断言或产品期限；阶段计数仍 **24/26离线已提交，剩D22/D23，固定PROGRESS-SCALE 80%**，不改主线台账。

运行：激活既有Rust1.98环境后，`CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/replay_rtc_frame_deadline.py`。只在git archive导出的私有源码首次真实Opus编码后暂停75ms；实际Session/Codec/UDP/ICE/DTLS以及原50帧双向loopback/撤权自检均复用，未另写期望业务状态机。两个变体使用同一原测试、同一源码快照与暂停位置：

- 保留原编码后帧龄检查：实际帧龄76ms、sent_frames=0，原自检预期exit101/1 failed/Bounds；保护在RTP writer前拒绝迟到帧。
- 仅在私有导出移除该检查：实际帧龄75ms、同一暂停点sent_frames=0，原自检exit0/1 passed；原双向语音/撤权断言也执行。这是故意制造的错误变体，不进入产品或工作树。

两个进程均UID1000/cap0/network none/read-only/128MiB/12pids/2CPU，无新依赖/外网/Root账户/设备/部署。实际本地镜像 `sha256:fc854d016cf274c1d3cb211d7763bc77755683849fa4546df79f38f5d9817c53`。源码、变体、程序及日志SHA256见[机器证据](../remote/member-d-rtc-deadline-evidence.json)，原程序未被测试变体替换。

本实验确认编码后真实时钟检查可以独立阻止迟到语音，不能据此宣称已经确定上轮ARM运行Bounds的具体分支或通过并发负载验收。上轮37/1/2及串行38/0/2证据保持；本轮ARM诊断另按其实际结果记录，不把控制实验和历史原日志混写。

已回查main `1e5cadd9c32c6576e77f10c9e65f0eab87076098`：A-27仍production_ready=false/crypto_wire_frozen=false，D22/D23缺生产安全会话/pin/签名域/Verifier/Sealer，不能用测试替身接生产。原DTLS密码依赖风险、C/Root/消费端正式安装和实机验收保持。原AGPL/上游作者保留，Codex辅助，未知实耗不编造。
