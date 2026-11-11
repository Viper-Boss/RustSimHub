# WEB5-RELEASE 交付（2026-11-11）

实现只读发行输入预检、三包固定源码复核和本机操作手册。没有安装/签名/健康/确认/刷写能力，没有设备/NAS/服务器、真实Owner/SIM/私钥或系统总线操作。整体 **81.5%，20/169，加权10.6%** 保持。

## 源码与范围

独立分支从真实 main `7535535bc5a95c42ff1ea70743b97be561f2049a` 开始；只新增 `tools/release-audit/`、`docs/release/web5-*` 和本交付报告，生产crate、已有构建器、其他包、workflow及全局看板未改。

- 工具产品源码：`3c0879226f771bdb1bf3ff076a86c412975d441e`。
- `git archive --format=tar` SHA256：`bf2de3767f1cae31b180c2ada7fb1a1bae5612eea34966858010edce94f46eda`。
- 最后报告提交仅文档，不称报告 HEAD 有新应用构建。
- 消费已交215 `610d7fa63b1a0f129a3f146c0595171868f8e059` 的合同；实际读取旧 ARM 制品 Source `8470608781b4ab772597cef0d767e012b594f257`、归档 `efebf0bf4619f48041c1e66a310b583d987cd7d97348d3bf9c81e79a5d848dc1`。**它们不是四包新源码的组合发行包。**
- 221的固件提取/通用身份初装/USB默认合同与本包手册对应；74旧包不复用作新组合证明。

## 实际交付

`audit.py` 默认完整 release scope；component 必须显式选用，成功仍 release_input_complete=false。核对外部固定完整commit、实际源归档和每个Git blob、profile/程序/ELF AArch64/GLIBC≤2.41、程序摘要及嵌入Source标记（BLE没有该标记）、六文件application-input、Root固定模板/默认OFF/角色/预算/原许可证与Go模块闭包。保留原Go实现和AGPL。

可额外验证真实format2应用和ProductMaintenance v1的原签名域：只用外部公钥/外部精确manifest摘要，逐字节绑定应用/Core/Root/source/profile/独立批准策略和有限目标；Root SOURCE_FILES与固定commit原定义交叉核对，绝不执行候选程序或调用安装器。完整scope缺签名Root/boot/modules/DTB/rootfs/固件及身份/默认OFF输入会拒绝。

系统层需要显式公开发行目录及额外审计index（不是生产刷写manifest）：锁定boot/modules原摘要，Android v0/有界gzip内核banner、DTB大小与ext4魔数；11个首启/固件/USB/应用overlay文件核对原源码和0755/0644；独立政策拒绝固定Owner密码/镜像身份/包含设备固件。完整索引和命令在工具README。

公开材料检查只读明确的公开输入，不进入真实用户私有目录，不输出匹配内容。有限检查私钥容器、敏感文件名和凭据/SIM/消息字段；显式public目录必须完整枚举所有非不透明制品，漏项/额外文件/链接/特殊节点均拒绝。公钥/证书正常允许。自用维护信任必须显式 self-use，公开发行拒绝该信任。

边界保持：**不证明编译来源、实际NSS、ext4包含所列staged树、opaque镜像无秘密、kernel/身份已启动或设备已健康**。随机32字节私钥改名、未知编码和任意私人正文不能靠模式扫描证明不存在。结果始终 installable/may_sign/health_verified/may_confirm/flash_allowed=false；只读输入辅助程序在私有临时目录制作有界快照后删除，不写输入/Git/系统。

`review_sources.py` 只读三包外部固定base/product/head/archive，核对Git祖先、分配文件范围及最后head仅报告差异；不导入或执行三包代码、不称不同源码制品已整合。新手册 `docs/release/web5-local-operations.md` 给真实CLI的初装、USB/Wi-Fi、备份验证、计划、应用升级/回退/recover、有限健康和Unknown边界；危险入口不会由预检调用。

## 实际验证与失败

- `python3 -m unittest discover -s tools/release-audit -p 'test_*.py' -v`：**18/18**。合成临时Git源归档、Ed25519应用/Root签名、同源/不同源、原profile/架构合同、漏许可证/篡改、symlink父/子/hardlink/FIFO、模式/JSON重复/布尔整数/预算/读取deadline、系统完整索引/漏项、维护信任公开拒绝、合成秘密拒绝；源码复核测试使用真实临时Git提交/祖先/归档/仅报告head。未访问真实私有目录。
- 实际旧847 ARM legacy13程序：component **通过**；product-recovery-health16程序：component **通过**，原Go闭包 **16模块**、Go1.27.3及许可证字节通过，并通过可信 `go version -m` 读取实际两份ELF的模块并集逐项对照；各自实际归档 **1660 Git blobs** 全核验。未执行候选ELF。
- 原847合成fixture应用+外部临时公钥的实际验签component **通过**；不使用/申请生产签名。
- 默认完整scope、未提供Root材料：exit2 `root_materials_external_inputs_required`；提供原unsigned Root输入：exit2 `root_material_inventory`；错误profile：exit2 `profile_or_architecture`；错误外部manifest摘要：exit2 `external_manifest_digest`。都没有绿色完整发行结论。
- 初次legacy实际检查因原生产者许可证保留0644/0600而被严格444假设拒绝；按原生产者合同允许这两种**无组/全局写**模式，未改生产者或放宽程序/模板模式。初次合成archive篡改测试实际抛原ValueError，修正测试接受原拒绝类别。初次源码复核工具误将console路径写为configurator，按实际crates/edge/src/web/console范围修正，并补真实Git回归。
- `python3 -m py_compile tools/release-audit/*.py`、`git diff --check` 通过。原始日志/详细JSON留 `.local/release-audit/`，不提交密钥/DB/原始材料。

## 三包复核状态与未运行

已实际运行本包 `review_sources.py`，只读核验三包最终完整源码、实际归档、Git祖先、分配范围和仅报告head，全部通过；原始工具结果直接交付为 `web5-release-source-review-20261111.json`：

| 包 / PR | 固定产品源码 | 最终报告head |
|---|---|---|
| console / #223 | `77abd18d786fb329f1a6f4529391d0ec1e168462` | `665f485f768d3d0fc71cd94dc6c3022f18b5336c` |
| firstboot / #224 | `ab874ea0fd3437fd9db63b8535a3e0124caeefca` | `ab36b762bce521d58567931c6b575fbb3cc06c30` |
| matrix / #227 | `6c35352ac9b0e4980c8545a84c74a69078fd1e40` | `2e8023e858ac8b40184acf2217cb8e91b9421cbd` |

console归档 `6e4415d101531e45171f3083a6b6c545d6a8f7ba246310a3290ef6b4abb6f183`，firstboot归档 `4d8477373f843dd17da0b140e073ba2bc21ad2b2674b44ee1513d99f7f1ebb6f`，matrix归档 `9272dc2c9beec3ffc8a2761d2435147096d2ef46e1a107a70cab3579a79d3c63`。matrix base为正常合入215后的04ac942，其余两包base为开工main7535535。三包生产差异分别11/16/11个文件，报告head只增加docs/deliveries。

最后只读消费matrix机器证据：18项真实模式检查（8支持/10拒绝）、原邮件3→4后SIGKILL连续三锁和新进程显式恢复6→7→9完整链、实际Core启动与回执保持、同源WASM/Chromium两套18组、ARM updater有限QEMU metadata均已有通过记录。实际中断inspection选旧应用的缺陷已修；schema9显式Root回退因缺少已验证fallback启动profile在写前Denied，SMTP5/AP自动确认仍不支持。该消费不表示发行审计工具重跑业务、四包已合并构建或设备健康通过。firstboot的实际ARM合同依赖仍是旧847程序，未冒充ab874新Core重建。

测试期间main更新到 `c5fd2d082e25d15535f249f11f5b7a4604427fea`；四包只读merge-tree检查无冲突。没有合入新main或把合并树当已测试产品。

未运行：四包合并后的同源全系统构建、完整新发行Root签名包及全部boot/modules/DTB/rootfs输入、镜像内文件/秘密核验、真实PID1/实际NSS/固件提取/身份启动/设备/USB/AP/SIM/业务外发/生产签名/epoch/健康确认/刷写。完整scope当前正确拒绝；已存在生产构建器和本机副作用不由本包改写。Cloud CI计费执行不记通过。

三包最终源码只读复核已完成；本包交PR226、三包交PR223/224/227，均为非草稿，未自行合并。后续由主线接纳后完成四包同源系统组合、完整发行输入及授权设备验收；不把未运行项目写为通过。
