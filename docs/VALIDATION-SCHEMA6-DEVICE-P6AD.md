# schema6业务交付与Wi-Fi恢复

2026-10-23。实际棒子已完成业务应用文件升级，正式首次认领尚未开始。整体保持77%；重复交付新版和准备证书不重复计分。

## 实际交付

- 应用及Native音频同源于`9528712c784d80c9326b99f46b1f72971b5714d2`，源码归档SHA-256为`9bbc71d42fb5b10d0a2346579dccc56f6d732828689c2879fed48b1b796a5d39`。
- 沿用设备原有发布公钥，重新签发epoch16/17/18、schema6..6完整应用包；独立OpenSSL核对全部签名和文件，以及schema5和实际篡改副本拒绝。
- 设备实际安装factory epoch16。旧factory epoch13经原子目录交换保留；本机另存旧文件并核对摘要。仅替换应用目录及update、edge-setup、migrate三个工具，Root网络与灯控维持原来源。
- 原始系统安装组成未改写：这是既有Linux系统上的应用文件交付，未刷整张新镜像、未写原始分区、未重启。
- 实际设备包验证、构建身份、Core受限自检，以及UID984/空cap/PrivateDevices/PrivateNetwork下合成Opus500帧和DTLS/SRTP测试通过；临时测试单元已移除。这不是PCM、真实通话或正式业务验收。

## 用户授权的既有Wi-Fi恢复

用户明确选择复用此前测试过的Wi-Fi配置。通过旧系统快照的只读文件系统工具恢复唯一WPA配置，原始快照保持；SSID、密码及原始配置只留私有目录。

新配置先以autoconnect=false发布到持久NetworkManager目录，验证连接成功后才保存自动连接。实际确认Wi-Fi和USB同时connected，USB配置摘要保持，普通gsm disconnected且无自动连接配置。连接成功不等于互联网可达或重新启动后重连已验收。

恢复过程中的两次输入检查失败及一次大文件复制会话超时均保留；超时后观察到复制完成且无打开者，后续复用副本，没有重新复制或改写原快照。

## HTTPS及媒体预备

使用已交付Edge工具的正式`--prepare-local-media`入口，以Edge账户、空cap和隔离文件系统生成私有TLS/config；证书仅通过固定身份SSH导出公有部分，独立解析SAN并核对DER/PEM摘要。私钥不离开设备。

当前只准备端点，未创建正式Core身份、业务库、安装完成标记或开启Owner窗口，Core/Edge/Media仍inactive。主要网页入口固定为当前Wi-Fi地址；额外USB媒体监听配置不等于USB地址的网页Origin已获支持。DHCP地址改变后的发现/端点迁移仍在剩余任务内。

新证书的Windows临时信任需单独获得授权；旧预览证书的授权不自动延伸到新证书。首次Owner窗口只有五分钟且不能靠重启重开，因此等用户明确准备好后，才进行正式初始化和启动。

## 私有证据及下一步

- `.private/schema6-originaltrust-verified-20261023-first.json`：三包原信任签名验证通过。
- `.private/schema6-business-delivery-20261023-first.json`：实际文件交付及受限硬件测试通过。
- `.private/schema6-business-backup-20261023-first/`：旧文件备份。
- `.private/wifi-profile-recovery-20261023-fifth.json`：只读恢复既有配置通过。
- `.private/wifi-restored-20261023-first.json`：实际Wi-Fi连接与持久配置通过。
- `.private/firstboot-tls-20261023-second.json`：证书和媒体配置准备通过；first仅本机依赖导入失败、未连接设备，日志保留。

不要重跑已消费的交付或Wi-Fi恢复helper。下一步按`initial-installation.md`使用相同端点请求完成可信私有交付，用户就绪后启动正式业务并认领；之后验证新系统短信、电话、音频和灯光。累计10010仍38，本轮新增电话/SMS均0，原卡普通数据关闭。关机/PMIC/SMD调查继续按用户要求搁置。
