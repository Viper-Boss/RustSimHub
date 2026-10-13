# 实时音频与应用更新开发边界

2026-10-14。源码已保存；这一页描述待验收实现，不增加进展点。整体保持约 48%，持续长任务目标约 60%。

`native/audio-alsa` 增加 str0m 0.24.1 音频会话：仅单路 Opus/sendrecv、UDP 私有 IPv4 host candidates，限制 SDP/行数/候选数、数据包和媒体队列。默认不记录音频。会话必须完成实际 DTLS 证书 fingerprint 校验，再将核验到的 peer_key 与核心票据匹配，才可 redeem、打开 PCM。先握手后签发短票据，不延长五秒票据。库使用 rust-crypto provider；dimpl 的证书生成仍静态引入 AWS-LC，不能称为全 Rust 依赖。所有 Rtc 变更后 drain 至 Timeout；授权/帧龄使用实际本机时钟，撤销后关闭。

`--rtc-self-check` 进行两个实际 UDP 回环端、ICE/DTLS/SRTP、50 帧双向 Opus 合成测试。此前 9 项原生测试通过，最新增加证书替换失败测试尚待构建。`--rtc-lan-peer PRIVATE_CONFIG` 的合成模式只送静音、不打开 ALSA；授权模式必须先匹配实际证书和核心 ticket，再由独立媒体线程开 PCM，核心心跳另由运行时续验。私有文件严格 UID/mode/nlink/NOFOLLOW/NONBLOCK；新输出不覆盖，均限定配置目录。仅开发诊断接口，不是浏览器媒体信令入口。TURN、公网、浏览器麦克风和真实双向听音未完成。

`crates/update` 增加签名整组应用目录、根权限 CLI 和版本选择器。签名绑定精确 manifest bytes 和专用域；固定板型/架构、单调 epoch、schema=5 兼容范围、文件允许列表、大小/哈希/ELF 检查。更新根目录锁定 release 公钥摘要。写候选并 fsync 后提交状态；第一次 choose 前持久化 attempted，未确认的第二次 choose 回到已确认版本，防降级 floor 不降低。confirm 绑定准确 manifest hash 并重新核验整组文件；确认必须由可信健康检查发起，CLI 不代替自动健康检查。

选择器每次应由一次统一应用组启动调用，不能让 core/edge 分别 choose。当前没有生产开机管理、下载器、授权远程更新接口、密钥轮换或完整掉电硬件测试。应用路径仅限自己目录，数据/身份独立；不写 boot、rootfs、NV、校准。system_ota_supported=false。初次安装须先确认 factory，才有回退目标。未完成完整镜像和恢复闭环，不刷分区。

构建产物区分 development 与 production；production core 必须自报 development_fixtures=false。部署核对每个二进制 source archive SHA 和文件 SHA。源包现从精确 Git blob 打包，独立于 Windows CRLF，并拒绝未提交源码。此前一次源包因 Cargo.toml CRLF 与 Git blob 不同，被验证拒绝；旧构建只作测试，不能作为最终部署证据。后续必须用匹配提交的新源包重建。

近期 NAS 编译器执行 125 项 workspace 测试，CI 8f087a2 的测试/格式/Clippy 成功（38438446921）。更新失败场景已包含签名改字节、数据坏哈希、跨板/schema、低 epoch、危险文件名/dev-fixtures、候选未确认重启、损坏 slot、撕裂 state.next、锁竞争/链接/错误确认；棒子安装/回退待实际执行。新测试与后续修改须再次验证，不能沿用此前结果。
