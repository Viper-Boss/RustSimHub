# 当前应用的完整系统候选组合

2026-10-17。将已验证的4e6fea6应用、同源Native媒体和签名factory接入锁定220包基础系统的新目录；不原地修改旧候选，不访问棒子或写分区。全部flash/release/recovery/board_boot门禁保持false，完整系统上电仍未证明。

首次在umask077下组合的隐私/来源审计通过，但实际以986执行ARM程序失败：exit126/Permission denied。原树12340项、SHA256 5d0b68a99563e5f0b57154a49a6af941358b4173617ca82b1bd8caa51b812452保持不变；私有system-tree-p6l-4e6fea6-runtime.json passed=false及原候选均保留，不冒充启动成功。

原因是mkdir(mode=0755)仍受调用者umask影响，新建程序上级目录变为0700。修正为仅对新建系统公开目录明确chmod0755、版本目录0700、公开来源元数据0644；既有基础目录权限保持，软链接/非目录及路径越界仍拒绝，不修改全局umask或递归放开权限。

42项Linux板级隔离测试通过，新增用例证明umask077下真实UID986可穿过程序上级目录执行，来源元数据可读，既有0700私有目录保持及链接/文件父目录/越界拒绝。私有composition-permissions-tests.log/exit0。

## 修正后的独立候选

组合源fba528e269dd104b38df77cb66f446595e252d78，应用仍4e6fea6、源归档d02625f65041de8207d8d3d81c20f623c1f9681e00d64abb3f565fee928b6012，基础树c6992751…及签名factory/同源Native保持各自来源。新树12340项，SHA256 `4324468e63920ce79bef8cd0da1a0a2e4bc7c201f6ea866a9da4641e5da8701e`。系统包含签名启动组、Native媒体与安装辅助，仍要求每设备明确配置媒体；持久状态为空。

- 六生产ARM程序以986、实际安装的Native以984运行，Root0555及摘要/构建来源匹配；无硬件/外部网络，Core self-check不执行硬件写入，CLI help通过。
- 在私有mount namespace中只读绑定树及选中factory，systemd-analyze verify覆盖Core/Media/Edge、选择/回退、firstboot、维护SSH、NM/MM/rmtfs依赖，退出0。仅Debian原装networkd/timesyncd的两条`!!`兼容提示，未修改上游单元。
- 执行及验证前后整树内容/mode/numeric owner/capability摘要保持。state为空，公开目录0755和私有versions0700匹配；全部刷写/发布/完整启动/恢复门禁仍false。

第一次重建包装脚本末尾因Windows换行退出127，但内部七程序probe已exit0/passed；原日志保留。随后用LF脚本对同一新树重新执行七程序与依赖/整树保持检查，`system-tree-p6l-4e6fea6-second-verify.exit`为0，`system-tree-p6l-4e6fea6-second-verification.json`passed=true。不要删除失败记录或重标首次树。

此轮是当前已测应用纳入通用候选并修复真实权限错误，不重复增加P6进度；整体仍73.5/100约74%。尚未在棒子用该整树启动，更未完整上电/刷写或恢复。WSL候选位置及后续内核源对照进展见HANDOFF.md。


## 后续发现：额外音频模块尚未纳入

22个额外音频模块已从固定KyonLi 93a71ee源码/headers与AirSimHub ea019485补丁重建，22/22逐字节等同设备现用文件。此前归档的25个qcom模块均为上游原装，不能代替这22个override；16个已加载模块的srcversion与只读捕获一致。本机私有Ubuntu GCC14.2.0-4ubuntu2~24.04.1交叉编译，补丁零fuzz；第二版19/22全字节相同，三差异仅__FILE__路径及build-id，第三版用-fmacro-prefix-map对齐原诊断路径后22/22相同。源码/headers输入未变、未安装或加载新模块。当前P6L通用树尚缺22个override，不能据旧系统通话通过宣布新系统声音可用；下一步在新候选纳入驱动并验证depmod/别名/预加载及冷启动。完整内核/initramfs独立重建、boot重打包和恢复仍缺，board.toml保持UNRESOLVED，所有刷写门禁false，不重复加分。 固定源码/输出锁见firmware/kernel/audio/modules/source.json。既有P6L报告和树摘要不改，离线七程序执行不涵盖驱动加载或真实声音。
