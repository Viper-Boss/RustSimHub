# C17 来源与下一阶段接口提案（待审核）

日期：2026-10-30。固定来源 main `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
C16 PR #98 尚待负责人审核。用户在知悉该状态后再次要求继续推进，本包仅独立提交C17来源、接口和最小样例供审；C18～C20实现仍待提案审核。

## C17 现有来源结论

现有 source-index.json 固定31个公开跟踪文件及Git blob/SHA256；cross-lock-checks.json 的19项检查通过，
其中7项确实读取跟踪源文件字节并与已有锁比较。它们与旧研究截面2249762逐文件相同，
不为相同代码重跑既有13项隔离测试，也不把旧执行结果改标为新主线执行。
完整内核归档、Image、实际DTB、681个原装模块、22个音频模块ELF和厂商固件本轮没有输入。

来源清单 inventory.md 分别列 release、源commit/tree、上游boot、板级零填充boot、Image、压缩内核、DTB、
模块归档和模块清单摘要。摘要对象不同不能混比；派生JSON不是另一套构建或发布manifest。
固件实际摘要、再分发许可保持未核验；不能按内核GPL推定闭源/设备固件许可。

## 拟提交供负责人审核的接口

本小PR交来源表、复用/缺口说明、以下输入/输出提案和最小失败描述；不修改既有锁或校验工具。
下一包允许目录只有 tools/kernel-artifact-audit/、docs/kernel-artifact-audit/ 及本人交付报告。

1. 默认列计划：不读取制品、不执行被检查程序、不下载、不扫描缓存或设备。只有显式执行参数及明确的公开/合成输入才做校验。
2. 输入选择已有 board/base/kernel/audio/module 源锁或显式合成夹具。路径映射只是调用参数，不能覆盖锁中预期值或声称发布受信。
3. 正式输入绑定固定源码提交与原锁字节摘要；合成输入必须标 synthetic，结果永远不升级为真实制品、许可、签名或硬件通过。
4. 制品仅来自调用方显式隔离目录，路径为有限相对POSIX名字；拒绝绝对路径、点段、反斜线/盘符、链接/设备/FIFO、重复/大小写冲突、未知版本及超限。
5. 内核/DTB/模块/固件逐类返回 observed、mismatch、not_verified，附实际读取对象的摘要/字节数及有限原因码。未提供的必需对象保留缺口，不能跳过后总报pass。
6. 摘要匹配只证明与指定锁或合成声明一致；报告另列 source_trust、firmware_license、boot、recovery、release_support。调用方自填 passed/healthy/verified 不能补齐证据。
7. 默认JSON输出到stdout；显式输出只能新建隔离报告，不能覆盖输入/既有证据。输出不包含实际绝对路径、载荷、密钥或主机/设备标识。

建议退出语义（仍待审核）：默认计划退出0且 execute=false、不含passed；执行后全部指定且必需的字节检查成功退出0；
必需证据缺失或不可核验退出3；明确不匹配退出4；输入类型/版本/路径/预算不合法退出1。
退出0始终不包含发布/安装/confirm授权；范围之外的硬件/许可验证保持not_verified。

建议预算（仍待审核）：清单1MiB；最多2048个制品条目；模块每个2MiB；DTB128KiB；
Image/boot各128MiB；总实际读取1GiB；FDT深度64、属性/节点总量有界；不解压任意归档或运行外部解析器。
完整源码归档核验复用原有专用工具预算，不暗中扩张本检查器或把tar归档摘要当成逐模块证据。

## 可复用函数与禁用入口

- tools/audio_modules.py：regular_bytes、modinfo、profile、verify_artifacts、loaded_rows。
  modinfo实际解析ARM64 ET_REL的.modinfo，不运行目标ELF；profile的22项固定集合不能拿合成期望值冒充。
  loaded_rows有意忽略集合外APR，C18总报告必须单列stock依赖未核验，不能继承忽略行为后称依赖完整。
  禁止调用 install/dependencies/run，不执行modprobe、depmod或读取运行模块。
- tools/rebuild-audio-dtb.py：verify_sources可复用5项固定源文件锁；rebuild会执行编译器，不在只读检查器调用。
- tools/verify-kernel-source-tree.py：source_tree/verify已有流式源树和界限；verify会写报告，只能经显式新输出使用，不能隐式运行其CLI。
- tools/verify-audio-boot-recipe.py：固定输入的checked_bytes和内存重构语义可参考；它的verify写报告，不应为简单计划隐式调用。
- replace-appended-dtb.py：parse/split_kernel能区分Android v0和末尾单DTB，但不验证DTB compatible。
  对板型核验需要实际读取有界FDT的根compatible，不能仅grep板名或用输入JSON自报。先提接口，审核后才实现。

## 第一阶段失败描述

下列是完整C19矩阵的待实现规格。当前只另附现有modinfo解析器的1个合成正例及3个最小失败字节样例，实际执行结果在本人报告；其余规格不能计通过：

| ID | 输入差异 | 必须观察的行为 |
|---|---|---|
| F01 | 未知schema或重复JSON key | 拒绝，不接纳“最后一个值覆盖” |
| F02 | ../、绝对路径、父目录链接、文件链接或硬链接 | 拒绝，不越出显式制品目录 |
| F03 | 一个字节超限、条目超限、截断/解析时增长 | 拒绝，有界读取，原输入和已有报告保持 |
| F04 | 正确哈希但实际DTB根compatible是另一板 | 明确board mismatch，不能只靠哈希/JSON板名成功 |
| F05 | 模块ELF为x86/ET_EXEC、截断节表或重复.modinfo身份 | 用真实modinfo解析拒绝，不执行程序 |
| F06 | 同release哈希不同、同hash类型对象错配、vermagic release不同 | 分别保留hash/object-kind/release原因，不合并成未知 |
| F07 | 声明依赖缺失、循环、_与-别名重复或缺APR实际来源 | 区分确定缺失、冲突与未核验stock依赖，不能从22项推全量681项 |
| F08 | 缺模块/固件、空必需清单、缺固件许可依据 | 明示缺口，不以空集合全部通过 |
| F09 | 人填passed=true或无来源healthy=true | 不当作字节/许可/签名或硬件证据 |
| F10 | 已存在报告或报告目标与输入别名冲突 | 拒绝覆盖，已有输入和证据不改 |

## 审核前置

负责人需要确认：既有锁的消费方式与显式合成模式、缺证据退出语义/报告字段、读取预算及FDT板型解析范围。
这些内容尚未批准；不接Root/部署/签名，不把尚未实现的C18/C19完整矩阵或来源提案计为制品/硬件验收。
本包基于最新main提交；C16未验收不计完成。负责人按本提案精确HEAD审核后，才进入C18～C20检查器实现。
