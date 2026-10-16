# 媒体工作进程的正式启动集成

2026-10-16源码实现，尚待新ARM产物、候选RootDirectory和完整板级启动验收。已有874网页短测发现约3秒媒体提前退出；本集成不宣称修复它。

可信初装器原四个参数后可显式追加`--media-lan [ADDITIONAL_IPV4 ...]`，最多三个额外地址。不可变Edge的`--check-local-media`先验证名称/私有IPv4/唯一地址；`--prepare-local-media`在同一个原子installation目录生成Edge配置、媒体配置和含摘要的记录。两份配置的主/额外地址来自同一输入。非媒体初装继续使用原命令，既有安装不能悄悄升级/降级或换地址/证书；要迁移需另行显式设计。

Root初装器在完成标记发布前，将已验证的media.json以UID984/0600的pending复制、比对、同步并发布到Media私有目录；存在的不匹配/链接/半成品拒绝。启动守卫要求Edge985和Media984两份文件一致，否则整组不启动。没有配置时Media服务跳过，管理页面继续采用原来不开放麦克风的配置。仅启动Media不会打开PCM；仍要求真实ACTIVE、Core票据和DTLS。

Media与Core/Edge使用同一选定签名目录，UID984/空cap、audio和IPC组，只允许五个固定ALSA设备，隔离D-Bus和其他角色状态。单独IPC目录为984:987/2750，Edge只读绑定；Core媒体授权socket沿用原路径。Media在Core之后/Edge之前，Edge等待984的socket。Media绑定Core/Edge的存活并归属app.target；媒体失败进入原来的一次整组恢复，预算已消费后明确停止整组，不保留部分存活状态。[systemd 257单元依赖语义](https://raw.githubusercontent.com/systemd/systemd/v257/man/systemd.unit.xml)已核对；实际条件跳过/依赖排序和强制退出还须在目标systemd验收。

组合器只在显式提供同源`--native-artifacts`、验证Native摘要/已通过worker构建及不可变安装器媒体能力时记录native_media_installed=true；不能因为旧签名包碰巧含ALSA程序就推断支持worker。所有刷写、完整上电、恢复和自动健康确认门禁仍保持false。

`test-boot-service-group.py --media-lan --failure-role media`将完整媒体配置和实际Media SIGKILL纳入候选根隔离验收；默认仍可回归无媒体安装。它不拨号/发短信、不初始化真实NSHSTATE或安装生产单元。Native终止诊断仅记录固定错误分类、帧计数/欠载与时序，绝不记录SDP、密钥、地址或音频内容；RTP缺失超时和Core许可规则未放宽。
