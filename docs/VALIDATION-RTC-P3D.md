# 独立加密 LAN 音频基础验收

2026-10-14；P3 增加 2 点，整体约 50%。本阶段只使用合成帧，没有打开 PCM、拨号、新短信或录音；不能代表浏览器或电话双向声音已经可用。

验收源码 aff5dcd，81 文件源包逐字节匹配。SHA256 `77a45fc434bdda13d6b5c1500d820696895a3a11c836b84954db9070fe4db3a9`。原生 9 项测试及 release 通过。二进制 5,320,088 字节，SHA256 `5bdacf29dafb9feb9fb9ee27bb122c9fb5902ba199897721a6ecc37e6961d46a`，自报源包 SHA 匹配。最终新增的证书失败/输出原子发布/断流检查不包含在这份产物，须重新编译验收。

棒子非 root rustsimhub-edge、PrivateDevices、限制内存/任务/运行时间，实际 UDP 回环两端完成 ICE/DTLS 证书匹配及 SRTP，双向各发/收 50 帧，合成解码峰值各 1492，错误 grant identity 拒绝，撤销关闭。内部网络循环 1402ms；systemd 运行 2.368s、CPU 1.231s、内存峰值 2M。握手与双端合成计算包含在样本，不能推算稳态媒体占用。

NAS 到棒子的 USB LAN：NAS 独立临时容器 User1000、只读 rootfs、无设备/能力、明确接口绑定和受限资源；棒子独立非 root 单元，PrivateDevices、禁止 D-Bus，只暴露自己信令目录。Offer/answer 经既有严格 SSH 维护通道传递，媒体走实际 UDP，未通过 SSH 转发媒体。两端均核对编译源 hash。

五秒双向：NAS/棒子各发送 250 帧，NAS 收 250、棒子收 249，应用队列丢帧各 0；DTLS verified、SRTP、权限撤销关闭成立。发送样本为全零，Opus 有损解码峰值各 1；验收限定 <=4 个 16bit 采样单位，不能要求有损静音解码绝对为零。首轮脚本使用绝对零错误判为失败；读取证据后修正条件并重新通过。一次初始传输脚本使用 SFTP x 模式没有写标志，已修正为 wx；两个失败都没有启动音频硬件或电话。

全部自有临时单元和容器已停止/移除，私有测试目录与日志保留。旧四服务 active，所有 PCM closed。native 和 Opus COPYING 在棒子 `/opt/rustsimhub/dev-rtc-p3/`；源码信令协调工具 tools/test-rtc-lan.py 只支持合成模式。私有证据：device-rtc-loopback-check.log、rtc-lan-synthetic-check3.log、rtc-nas-failed-stats.log、rtc-device-failed-stats.log、rtc-native-artifact-ready.log。

后续：将实际核验的 DTLS peer_key 接到 core 票据和真实 PCM，短时 10010 下行经 NAS 解码并检查静音上行；浏览器媒体信令、真实麦克风、TURN、公网、切网和听音仍未完成，media_available=false。
