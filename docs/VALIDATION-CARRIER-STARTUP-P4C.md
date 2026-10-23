# 新系统联通语音启动适配补全

2026-10-23。整体仍 **78%**。后续已完成固定源码构建、Root交付和普通冷启动：来电与10010拨出ACTIVE已恢复，但音频路由初始化仍缺、声音未通过；最新实际结果见 [P3K](VALIDATION-AUDIO-STARTUP-P3K.md)。下方“尚未”保留为候选准备阶段记录。

用户确认当地只使用4G/5G，取消2G/3G对照；没有修改允许制式或频段。旧NasSimHub尚未重新连接新系统，其离线提示不用于判断SIM注册。

## 现场证据

- 新系统10010试验只有dialing→terminated，无ACTIVE、无PCM；实际累计41。用户从外部拨入也无法接通。
- 只读NAS信息显示LTE、CS/PS attached、网络声明支持语音/IMS，但不等于设备IMS已注册。IMSA查询返回InvalidServiceType，保留为未知。
- 旧AirSimHub固定提交`ea019485e5d890a96acae357c52b94c746de837f`的`docs/UNICOM-VOICE.md`记载同板型需要运行时PS优先；旧快照也安装了对应carrier服务。新系统当时读到CS优先且没有该服务。
- 从旧快照以只读方式提取原已安装的Go有限工具，临时将2→3并用独立qmicli读回PS优先。此前版本别名与真实版本字符串不一致、以及`/run`不可执行，均在写入前退出；没有放松挂载策略。**仅这一项恢复后，用户拨入仍失败，观察窗口内没有新的Call对象。**
- 继续核对发现旧快照的`80-nassimhub-local-volte.conf`实际覆盖rmtfs启动入口，而新系统仍运行`rmtfs -r -P -s`。本机保留的私有profile四个文件、所有者/权限及SHA256SUMS均通过，尚未激活。

旧原始工具只是临时恢复手段，明确为Go，不作为Rust成果；设备上的临时副本须在Rust工具核验后移除。没有刷写、重启remoteproc、写硬件NV或启用普通数据。

## 新候选实现

Root网络组件增加三个固定本地命令：`--voice-domain-get`、`--voice-domain-ps-preferred`、`--voice-domain-cs-preferred`。仅Root/aarch64/UFI003可用，连接已有Root qmi-proxy并分配/释放自己的NAS client；不开放原始端口、任意QMI或网页执行接口。

Rust实现限制帧/长度/事务/服务/client/响应类型，拒绝重复TLV和失败结果，读写按绝对截止时间终止。仅接受2/3基线，仅发送POWER_CYCLE的voice-domain TLV，写后读回，结果不明不重试。不改变选网、频段、APN、PDC/MBN或SIM。

`rustsimhub-carrier-voice`保留旧策略的SIM本籍运营商白名单，活动/未知呼叫时不改。先以0600写入本次上电的动作意图，丢回复及服务重启后不重复设置；只恢复自己曾接管的CS基线，不接管原有PS偏好。策略对未知SIM/配置保持不动。

`rustsimhub-rmtfs-profile`只在本机显式Root opt-in及板型/物理NSHSTATE/目录/文件权限/完整摘要均通过时选择**只读**私有shadow。没有标记时沿用原分区只读模式；显式启用但材料损坏时失败，不静默回退。镜像不带任何profile或启用标记。新增守卫`--read-only`只允许在ProtectSystem使父挂载只读时做相同物理身份核对；默认写入守卫及SSH专用写入规则不变。

## 已完成验证与尚缺

- 网络组件12项Rust测试（其中新增协议6项）及Clippy通过；固定NAS请求向量、错误事务、格式/长度/结果拒绝、已知基线、绝对截止和帧数上限覆盖。
- 两个真实启动脚本的12项合成回归通过：只读选择、篡改/路径/权限/板型拒绝、未知SIM/基线、活动/异常目录、预存PS不接管、丢回复不重试及换卡恢复。首次直接运行继承umask导致动作标记0644，已在脚本显式umask077修正；没有放宽600权限检查。
- 系统工具154项回归通过，1项环境条件跳过；包含新增只读物理守卫的正反例及原写入隔离测试。
- 七组网页回归通过。另修正未接通时的结尾提示，避免“挂断成功”被误当作“通话接通过”；不会提前打开PCM。

尚需固定源码ARM构建、正常维护部署、现场只读预检查、备份/启动依赖核验；再由用户普通拔插验证profile与PS优先实际冷启动，随后才做来电/10010听音。**未在运行中重启基带，不恢复已搁置的PMIC/SMD试验。**

旧配置参考：[AirSimHub联通语音适配](https://github.com/Viper-Boss/AirSimHub/blob/ea019485e5d890a96acae357c52b94c746de837f/docs/UNICOM-VOICE.md)。背景架构可参阅[OpenIMSd作者的VoLTE组件说明](https://www.openimsd.de/pages/architecture.html)；其中通用架构不能替代本机profile和运营商验收。

私有入口：`runtime-voice-domain-20261023-third.json`、`incoming-after-ps-20261023-first.json`、`rmtfs-profile-comparison-20261023-first.json`、`old-rmtfs-unit-20261023-first.json`、`carrier-voice-preparation-20261023-first.json`。原始profile、镜像、日志与恢复工具仅放私有目录。
