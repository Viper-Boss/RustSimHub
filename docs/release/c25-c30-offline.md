# C25～C30 离线实现与接线材料

执行人wnfhsj88；从main `5c5fdbe0979304572ec87618e9ba3b8c8bb47fac`接手。步骤归属见[转交记录](../team/TRANSFER-C25-C30-TO-WNFHSJ88-20261103.md)，原C01～C24作者和审核保留。工具/接口是本次候选，需Viper-Boss按交付报告的完整HEAD审核；本页不冻结新生产格式，也不签认设备支持。

## 材料与复现

统一入口 `python3 -m tools.offline_delivery OPERATION` 默认仅输出计划，不读输入、不创建目录。显式`--run`才执行离线检查或写入**全新**暂存目录。没有安装、签名、引导、网络连接、服务启动或任意命令执行入口。所有输出的`authorization_verified`保持false。

| 步骤 | 已实现的范围 | 仍须主线完成 |
| --- | --- | --- |
| C25 / UPD02、03 | 有限Root角色/目标、UPD01源码契约摘要、ELF头/架构/权限/大小/摘要检查、固定配置、干净材料树与双次字节/权限比较 | 完整发行rootfs、可信基础系统/包锁、各程序真实可复现构建、Root维护授权、签名/安装/choose |
| C26 / UPD05 | 真实签名和载荷验证后构造候选；精确来源/进程/Root/data/新鲜度关联；Linux实际exe/status探针 | 可信Root生产者、活的维护锁/事务、一次性challenge消费、健康政策与confirm接线 |
| C27 / UPD08 | 复用既有kernel/DTB/模块检查器；可复现外部newc initramfs构建/静态解析；可选完整内核cohort门禁 | 缺失真实材料、启动时依赖、冷启动/掉电/恢复路径和分区目标验收 |
| C28 / NAS03、04 | 独立正常客户端的有限控制方法、Core v2字节契约、真实媒体Permit/Pump音频端口 | 正常客户端批准、真实NAS/UI集成、DTLS已验证媒体提供者、跨主机时钟/传输与双向听音 |
| C29 / 发行动作组 | 固定Git来源索引/错SHA或缺项拒绝、公开能力/缺口和只读执行计划 | 真设备事实、REL01支持冻结、REL06最终签认 |
| C30 / REMOTE02 | 有限TURN/DNS/TLS/systemd/预算与清理模板，离线生成和逐字节校验 | 私有资源、真实凭据/证书、部署/防火墙/负载及停机验收 |

### C25 输入与输出

Root材料仅接受三种已落地路径：`core-launcher`、`updater`、`setup-ap`，目标均在`usr/libexec/rustsimhub/`下，0755；不会把应用槽程序混成Root材料。六份setup-ap配置、完整forwarding材料集及基础Core单元从固定来源复制，Core转发drop-in按既有公式生成；CRLF归一为LF，与`admission.rs`已有SetupAp必须包含forwarding的契约一致。forwarder.json为0600，其余配置0644。NKN/BLE尚未获准的Root候选不能夹带进这个角色集合。

输入JSON严格字段：`schema:1`、`board:"ufi003-v02"`、`arch:"aarch64"`、`contracts`及`files`。`contracts`必须等于`tools.offline_delivery.rootfs.contracts()`返回的现有UPD01/launcher/配置源码摘要；每个`files`项仅`role/path/sha256`。调用者提供隔离构建的AArch64 ELF，不接受自选目标、Owner数据或凭据文件。单文件128MiB、总512MiB；拒绝软硬链接、非普通文件、setid/危险写权限及检查期间变化。

```sh
python3 -m tools.offline_delivery rootfs --run \
  --manifest /private/public-materials.json --inputs /private/clean-build --output /private/new-materials-1
python3 -m tools.offline_delivery rootfs --run \
  --manifest /private/public-materials.json --inputs /private/clean-build --output /private/new-materials-2
python3 -m tools.offline_delivery compare-rootfs --run \
  --inputs /private/new-materials-1 --other /private/new-materials-2
```

这是**Root材料层**，不是可启动整机镜像：`complete_bootable_rootfs=false`。不调用chown、mount、包管理器、chroot或程序本身。目标uid/gid=0只是计划字段，实际暂存目录归运行用户；目录0700。输出独占创建，文件fsync/摘要回读后才创建COMPLETED并移除INCOMPLETE，已有目录一律拒绝。中断目录不自动续用或清理；不能宣称整目录原子发布或真实掉电恢复通过。

### C27 构建链

先按[固定内核重建说明](../rebuild-base-kernel.md)使用既有`tools/rebuild-base-kernel.py`，再按[C18检查器](../kernel-artifact-audit/usage.md)检查既有固定kernel/DTB/modules，避免重新定义版本或依赖语义。本次不重复历史181.3秒/681模块的真实重建。

外部initramfs输入为`{"schema":1,"files":[{"path":"init","source":"public-init","sha256":"实际摘要","mode":"0755"}]}`。显式提供公开、审过的init及所需普通文件；仅0644/0755，拒绝链接、设备节点、父文件冲突、重复路径和不完整档案。排序、inode、uid/gid、mtime和padding固定，内存中构建/校验newc，**不解包或执行init**。真正首启的mount/根分区发现/固件准备不由本工具猜测。

```sh
python3 -m tools.offline_delivery initramfs --run \
  --manifest /private/initramfs.json --inputs /private/public-init-files \
  --kernel-manifest docs/kernel-artifact-audit/pinned-input.json \
  --kernel-inputs /private/locked-kernel-artifacts --output /private/new-initramfs
```

只在既有检查器pinned模式及记录的源码锁完整通过时设置`kernel_cohort_verified=true`，报告记录该检查结果摘要。未给内核输入时仍可验证档案格式，但cohort保持false；合成cohort另记synthetic_only，不能提升为pinned；合成材料不能当成固定板子制品。`boot_verified`始终false。压缩、boot.img封装、加载模块和刷写不在入口内。

## C29 能力与只读验收计划

[索引](capability-index.json)逐项绑定完整Git commit、公共文件路径和实际blob SHA256。校验器拒绝漏能力、重复、错摘要、非普通Git blob、私有/越界路径和自报设备通过；成功只证明这些来源字节，不证明文档内结论已由工具复验。

```sh
python3 -m tools.offline_delivery evidence --run --manifest docs/release/capability-index.json
```

| 动作 | 可公开事实 | 当前候选设备结论 / 下一项只读检查 |
| --- | --- | --- |
| FWD22 | 现有Core转发、SMTP及隔离回执有源文件/历史离线证据 | unknown；先核对关闭状态、批准渠道及固定出口；不发真实邮件/短信 |
| UPD10 | 应用更新器、Root准入与本次材料/健康候选 | unknown；核对实际选择/boot/信任/只读inspect-state；候选一致不能confirm |
| REC05 | 既有恢复矩阵和加密身份文档 | unknown；核对匹配身份/数据库/恢复域及可读备份；不清空或还原当前数据 |
| REL01、04 | 既有内核锁、构建及安装资料 | unknown；核对板号/DTB/模块/许可/分区目标和独立恢复入口 |
| BLE0-01、02、04 | 既有Gatt/Core/Root候选及隔离失败矩阵 | unknown；先核对真实HCI/BlueZ/设备权限，缺HCI和RF事实不宣称支持 |
| BLE6-04 | 配置器候选及有限恢复流程 | unknown；真实浏览器/射频/断电/旧版本OTA仍须获准实测 |
| NAS03、04 | 新正常客户端与typed音频端口 | unknown；旧页面/旧PCM格式未固定，不宣称全部旧UI兼容 |
| REMOTE02 | 本次私有中继模板；既有实际TURN客户端 | unknown；证书/地址/资源仍待私有配置，未部署 |

安装/回退/使用资料复用[REL05手册](../manuals/README.md)、[初装](../initial-installation.md)、[备份维护](../manuals/backup-and-maintenance.md)、[支持证据](../manuals/support-evidence.md)，不另造相反命令。安装前由主线核对新鲜只读基线、可读备份、恢复入口、可信制品及精确目标；失败保留原槽、原数据、Unknown与回退预算，不能擦floor/换Owner“恢复成功”。日常使用先检查已授权会话/线路/模式，未知操作只按原ID查询，不自动重拨或重发。上述是执行计划，本次没有执行设备安装或回退；历史有限实机结论仍只对其原版本/场景有效。

## C30 私有中继模板

示例公开输入见[有限配置](relay-example.json)。仅域名和有限端口/配额/寿命，无秘密字段；DNS名称只接受ASCII合法标签。固定coturn `4.6.1-2`，TLS控制/UDP relay，与既有D客户端一致；拒绝TCP relay和私网peer，TLS最低1.2，显式禁用旧TLS。

```sh
python3 -m tools.offline_delivery relay --run \
  --manifest docs/release/relay-example.json --output /private/new-relay-template
python3 -m tools.offline_delivery check-relay --run \
  --manifest docs/release/relay-example.json --inputs /private/new-relay-template
```

输出包含TURN、DNS、systemd模板及证书/清理计划，地址、secret、证书/私钥路径保留`@PRIVATE_*@`，`configuration_complete=false`。逐字节校验会拒绝篡改、加白名单和权限变化。它故意不是可启动配置，不能用生成成功称部署成功；真实填充仅在仓库外进行。预算128MiB/16任务/200% CPU/5秒停止、每会话64000B/s/服务器256000B/s、10秒建立、最多8 allocation及600秒寿命是提案上限，未做真实负载测量。停机/清理只列固定实验unit和运行目录，工具不执行其中动作。

## 验证与来源

`python3 -m unittest tools.offline_delivery.test_delivery -v`验证实际模块与临时文件/Git仓库，包括路径/链接/FIFO/权限/预算/重复JSON、错误版本/摘要、输出冲突/中断、newc格式/完整性、证据错SHA/漏项与模板注入。内核cohort失败分支使用外部检查器故障夹具；既有真实检查器回归另行执行，不把mock失败夹具称真实内核验证。

健康/NAS接口与失败矩阵见[健康接线提案](health-candidate-v1.md)、[正常NAS候选](../../tools/nas-adapter/control/README.md)及本次完整HEAD交付报告。新代码由Codex辅助编写，AGPL-3.0-only；复用原仓库UPD01、C的C18检查器/手册、D的媒体Permit/Pump与TURN客户端，保留各自原作者/许可。未下载或复制第三方实现。整体81.5%、20/169和加权10.6%保持。
