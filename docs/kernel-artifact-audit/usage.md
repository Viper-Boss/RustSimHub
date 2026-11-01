# C18 只读制品一致性检查器

实现入口：[audit.py](../../tools/kernel-artifact-audit/audit.py)。消费已合并的[C17提案](proposal.md)，
只观察显式输入，不修改原锁、制品、设备或生产更新器；不提供安装、签名、确认或整机支持授权。

## 运行

需要 Python 3.11+。默认计划在 Windows/Linux 均可读取；真正检查要求普通用户 Linux（本项目在 WSL2），
以使用文件描述符相对寻址与 O_NOFOLLOW，不在 Windows 上提供弱化的链接检查。

```sh
# 不读取清单、制品或创建报告
python tools/kernel-artifact-audit/audit.py

# 普通Linux用户；显式提供公开/合成清单及隔离制品目录
python3 -B tools/kernel-artifact-audit/audit.py --run \
  --manifest docs/kernel-artifact-audit/pinned-input.json \
  --artifacts /explicit/isolated-artifacts

# 可选：在制品目录外的既有隔离目录中新建报告；目标必须不存在
python3 -B tools/kernel-artifact-audit/audit.py --run \
  --manifest INPUT.json --artifacts /explicit/isolated-artifacts --report /explicit/reports/new.json
```

示例路径为占位符，不是实际部署位置。工具不扫描目录寻找文件、解压归档、下载固件、
执行被检查的ELF/脚本、调用modprobe/depmod/编译器或访问NM/设备。
缺少映射或文件会给出缺口，不能把没有制品时的计划输出当成成功检查。

## 两种模式

### 固定源锁模式

[pinned-input.json](pinned-input.json)只有 schema、mode、source_commit 和 mappings。
source_commit 是C17固定的 **RustSimHub** 截面 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，
不是上游内核的93a71ee提交，也不是每次新增文档后随意更新的当前HEAD。
工具用C17来源索引验证消费的7个原锁/解析器字节，再从原board/profile/audio/modules锁推导期望值。
调用方只提供24个对象的相对路径，不能覆盖期望hash、长度、板型或模块元数据。

- kernel-image：原未压缩Image的固定长度和摘要，ARM64小端头；不把image_size内存范围等同文件长度。
- audio-dtb：音频DTB固定摘要及实际FDT根compatible，与原board compatible列表逐项比较。
- audio-各模块名：22个原锁模块的长度、摘要、真实ELF .modinfo、vermagic release与声明依赖。
- 681原装模块没有本次逐模块输入，外部stock依赖（包括APR）保持未核验。
- 私有/厂商固件的公开清单和再分发依据缺失，保持明确缺口，不自动去设备或维护缓存补全。

因此即使这24个对象都匹配，当前固定模式仍因stock/固件证据缺口返回3；
这不是检测器故障，也不是允许忽略未核验项发布的开关。原锁的历史标志不被改写。

### 合成模式

顶层字段严格为 schema=1、mode="synthetic"、kernel_release、board_compatible、entries。
每条entry严格为 id、kind、path、bytes、sha256、metadata；不接受passed/healthy等额外字段。
kind仅支持 kernel_image、dtb、module、firmware。四类必需对象缺一保持未核验，空集合不能通过。
id为最多64字节的小写字母/数字及点横线下划线；path为有限ASCII相对POSIX路径。

| kind | metadata |
|---|---|
| kernel_image / dtb | 空对象 |
| module | name、vermagic、depends（字符串数组）、srcversion；必须与实际.modinfo相符 |
| firmware | license：有限声明标记或null；null保留缺口。非null也只是调用方声明，报告不将许可验证为通过 |

bytes为正整数，sha256为64位小写十六进制。所有哈希与元数据都来自合成输入，
即使检查返回0，source_trust仍为synthetic_only，不能升级为可信生产来源或真实发布证据。
真实检查器的完整合成输入生成示例见[test_audit.py](../../tools/kernel-artifact-audit/test_audit.py)的setUp及module_elf/dtb。

## 输出与退出码

| 退出码 | 含义 |
|---|---|
| 0（无--run） | execute=false的计划，不含passed或检查成功声明 |
| 0（--run） | 指定必需字节与当前支持的结构检查完成；byte_checks_complete=true，仍不授予安装或发行资格 |
| 1 | 输入版本/字段/路径/文件类型/链接/预算等非法，或输入IO失败；有限reason，不回显主机路径或原始异常 |
| 3 | 必需对象、映射、依赖或固件许可声明证据缺失，not_verified |
| 4 | hash/长度、实际二进制结构、板型、模块元数据/release不匹配，或依赖环 |

每条observation输出有限id、kind、observed/mismatch/not_verified、实际字节数/hash和有限原因。
缺文件不伪造观察值；缺证据和明确不匹配同时存在时4优先，缺口列表仍保留。
未解决依赖按实际已核模块图列出；名称中的下划线/横线按模块惯例规范化，拒绝别名冲突/重复依赖与环。
输出始终保留 authorization_verified=false，firmware_license/boot/recovery/release_support=not_verified。
Kernel release通过固定对象摘要及模块vermagic绑定，不声称从原始Image头读取出运行内核版本。

## 文件与解析预算

仅Linux普通UID执行。目录逐组件openat/O_DIRECTORY/O_NOFOLLOW，文件O_NOFOLLOW/O_NONBLOCK，
拒绝链接、多硬链接、非普通文件和读取期间变化；不跟随用户维护目录中的链接。
显式目录不能为根、/dev、/proc、/sys或其子树。不递归枚举任何制品目录。
只读取清单列出的相对名字；调用方须提供公开或合成输入，本工具不承担授权读取私人资料的职责。

清单≤1MiB，≤2048条，累计实际制品字节≤1GiB；Image文件及声明内存范围≤128MiB，
DTB≤128KiB，模块/固件每项≤2MiB；无归档展开。超出范围的制品须先由负责人审核扩展范围。
FDT仅接受v17/last-compatible16，检查头/保留区/区间、最多8192个token及64层深度、根compatible；
不是完整设备树语义验证或硬件匹配证明。模块结构复用原audio_modules.modinfo，不运行目标模块。
格式依据：[Linux ARM64头](https://docs.kernel.org/arch/arm64/booting.html)、[DTSpec FDT格式](https://devicetree-specification.readthedocs.io/en/stable/flattened-format.html)。

报告目标必须不存在且位于制品目录之外；新建0600，O_EXCL阻止覆盖或并发替换。
输出目录不自动创建，不覆盖原清单、输入、旧证据或其他人报告。stdout仅输出脱敏结构化结果。

## 验证与保留边界

```sh
python3 -B tools/kernel-artifact-audit/test_audit.py
```

测试运行真实audit.main/inspect及原解析器，在新临时目录里构造匹配/错板、坏Image/ELF/FDT、依赖、
缺证据、非法JSON/路径、symlink/hardlink/FIFO、预算和报告覆盖场景；无真实制品、Root或硬件操作。
本次执行、精确提交/tree和未运行项见[成员报告](../deliveries/member-c-nas.md)。
C18交付只读实现及必要验证；C19固定样本及C20离线交接见[矩阵与补验清单](delivery.md)，不凭测试数量宣布硬件或发行验收完成。
本轮没有核验真实完整内核归档、boot、681/22模块字节、厂商固件许可、实机启动/恢复或正式签名发行。
