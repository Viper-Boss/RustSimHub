# 最新 GitHub main 复测：实际结果与阻塞清单

本报告回答“目前固件进度到底卡在哪里”。基准是重新从 GitHub 回读的最新
`main`：`de663743bd726e77f7c560139d0f91ea78e8fe57`，提交时间
2026-11-16T10:21:31Z，tree `897b7b485ce8b3108d359d859dd2384efba78df1`。
测试结束后再次回读，main 未变。待合并分支的结果单列，不当成主线结果。

目标使用新建干净 checkout，源码只读，前后 source digest 都是
`8d6570bccd203e9a733cc803298679fca9b1ac4d91ff7ab3540d435f9993fde5`。
本轮未修改 main、全局看板、已有身份、开机策略或生产服务。

## 一、本轮真实复测结果

| 范围 | 最新 main 的实际结果 |
|---|---|
| product-release profile 工具测试 | 18 通过 |
| product-integration 验证器测试 | 12 通过 |
| release-audit | 18 通过 |
| kernel-artifact-audit | 21 通过 |
| second-board 工具 | 12 通过 |
| recovery 工具 | 7 通过 |
| rootfs 工具 | 36 通过 |
| performance 工具 | 11 通过 |
| 已列入主线清单的来源检查 | `--require-complete` 返回0，清单本身只有18个输入 |
| usage-stats，含 transaction-candidate | 92 通过、0忽略，返回0 |
| 原缓存时钟失效回归 | 查询中失效、有/无缓存、后续分页失效等3个专项均通过 |
| product-install harness | **编译失败，E0433，缺 tempfile，返回101** |
| update 的 health::tests | **6通过、1失败，返回101** |
| product_materials_rootio | 常规11通过；默认2项ignored保留原记录 |
| 上述2个专用tmpfs用例 | 在独立私有1MiB tmpfs分别实跑，各1通过、0忽略 |
| product_materials_package | 16通过、0忽略，返回0 |

八组工具测试合计135项。工具单测通过不代表构建了可启动固件、装机成功或
真实业务验收。没有运行全仓蓝牙/AirSIM/native音频或热点功能测试。

审计控制器写完报告的退出0仅表示报告生成结束，**不能代表整体测试通过**。
两项真实测试命令返回101。公开摘要明确 `overall_status=failed`、
`overall_exit_code=2`，完整失败及后代清理结果保留，源码前后干净且稳定。

机器证据：

- [main-summary.json](../offline-b/main-audit-20261124/main-summary.json)：原报告SHA256
  `a3b6281bca4d08a4309dd1c72397c5dd19c9974b9a771817d41630bdfff2f2b4`。
- [statistics-summary.json](../offline-b/main-audit-20261124/statistics-summary.json)：原报告SHA256
  `164212739aef65f6fe0099c65c9c10cfb6d38d1be0558a94a66aacd4535ab254`。
- [rootio-special-summary.json](../offline-b/main-audit-20261124/rootio-special-summary.json)：
  直接使用本轮main编译的原测试程序独立快照，2955536字节，SHA256
  `48e25f718c4eb2344513f76ab8e53eb03590a0aaf641aaf9ce7d542aff2d541f`。
- [github-snapshot.json](../offline-b/main-audit-20261124/github-snapshot.json)：主线、PR、CI及新旧固件锁实际状态。

环境为本任务自建Debian13/WHPX VM、Rust1.98、原锁离线vendor。普通测试是
builder用户、私网/tmp/IPC/devices、零能力、NoNewPrivileges、只读源码与工具，
仅任务缓存/输出可写；主审计768MiB+1GiB swap、64任务、1CPU、1500秒，统计
512MiB+512MiB swap、48任务、660秒。所有目标缓存都是本轮新建；不同源码
或负对照不混用Cargo目标目录。使用B控制器c951783记录目标main，原进程后端
两份源码与目标main逐字节一致。各子命令维持有界输出/时间和原pidfd/subreaper回收。

两个tmpfs专项另在VM的Root私有挂载命名空间，仅CAP_SYS_ADMIN、64MiB/30秒
单位中运行；原测试自行核对专用tmpfs/只读状态，未填满普通磁盘。未改默认
忽略标记，也未把原11+2ignored报告改成单次13通过。

## 二、确定复现的主线测试问题

### 1. 安装回归还不能编译

实际命令：

```sh
cargo test --manifest-path tools/product-install/harness/Cargo.toml --lib --locked --offline
```

主线harness包含了`crates/update/src/product_materials/rootio/owned.rs`的测试，
但没有声明测试所需`tempfile`。本轮在该文件约186/200行复现E0433。
这是测试链路故障，不是已证明设备安装器的生产运行故障。修复已在
[PR278](https://github.com/Viper-Boss/RustSimHub/pull/278)，尚未合入main；不能因
分支已通过就宣称main安装回归已通过。

### 2. Root健康真实子进程探针的单测失败

```sh
cargo test --lib -p rustsimhub-update --locked --offline health::tests:: -- --test-threads=1
```

最新main的`kernel_probe_reads_real_executable_and_refuses_missing_process`在
`crates/update/src/health/tests.rs:232`对`Err(Invalid)`执行unwrap，实际6过1败。
失败被保留，不能笼统归因于没有设备。此前相同健康源码也出现一次失败，
16次暖态单项复跑则全部通过；首次错误的精确成因仍不能只凭重跑确定。

新的独立候选[PR282](https://github.com/Viper-Boss/RustSimHub/pull/282)仅增强测试
夹具的真实exec就绪观察，保留生产探针的一次性fail-closed规则。实际受测
f3384b3有8项通过、0忽略；真实管道门闩负对照在移除等待时确定断言失败，
Rust2021格式和严格Clippy通过。该结果属于候选，尚未合并；不能称生产Root
服务故障已修复，也不能代替当前main失败结果或真实健康授权链验收。

最初PR41的统计缓存时钟P2，当前main已包含修复和本轮通过的回归，已经不是
这次复测的阻塞。workspace也同时包含mdns和usage-stats，旧Cargo.toml冲突
不能继续当作当前阻塞。

## 三、集成与生产功能还卡在哪里

### 3. 交付尚未进入主线，完整组合也未验收

结束时共有12个非草稿PR和9个草稿。12个非草稿分别为270～275、277～282，
对当前main都可文本合并；不存在当前已确认的文本冲突。`mergeable=true`
不代表技术审核/运行通过，不证明这些PR同时组合没有接口或行为问题。

主线仍停在11月16日。A2～A5的NKN/远程、短信NAS、USB和验证交付，以及B的
runner、来源锁、安装修复、时间戳夹具和SDK/ARM证据，均不能当作已入main。
其中280/281主要是实际构建/测试证据，277主要是盘点，不能按PR数量换算新增
功能。草稿268是完整CheckInTransport的关键后续，不能自动接管/合并。

main的来源清单验证通过的是它列出的18项；PR271扩展到32项，也仍只是该
固定来源截面的契约。A/B后续代码整合后需再次更新并验证完整输入，重新
构建/复验统一HEAD。之前6746473b组合的33组结果或c951783的15项恢复矩阵，
都不能改标签成为最新main或未来全PR组合的通过。

按用户既有边界，本窗口不自行合并、改别人分支、改作者或全局进度。

### 4. NKN统计还有生产接线缺口

main已支持显式只读汇总查询和独立参与同意的持久保存；
`docs/nkn/usage-participation-integration.md`仍明确`reporting_available:false`。
“许可已保存”不能说明已经签名/提交或链上确认。

`docs/nkn/usage-query-integration.md`列明尚需：独立身份、真实纯交易准备/摘要、
持久CheckInManager与受限签名端口接线；原摘要下的单次提交、掉线/崩溃后
查询确认；关闭/换Owner后的迟到结果代次验证；长期/续订政策单独审核。
现有Root最多300秒授权不能擅自扩展成永久后台权限。这些包含代码和契约
工作，不全是等待设备；真实RPC/链上行为则仍需要后续明确授权。

### 5. 组件通过尚未闭合成最新完整发行profile

PR280的固定SDK构建/许可证/纯向量及PR281的3个Rust ARM组件/QEMU元数据是
真实证据；后者不等于完整固件或12程序+音频的完整应用组。

main的`tools/product-release/release.py`有6种固定profile，要求完整同源程序、
匹配ARM/音频sysroot、实际程序元数据和四字段NSS/角色快照。产品模式另有
SDK/Root/可选恢复等输入，源码/协议/schema/outbox必须一致。当前没有本轮
最新main完整profile的产物与联动结果。窗口对native音频/蓝牙的排除仍有效，
不能用缺件或旧程序替代、发明可用profile或放宽校验。

大部分profile的自动健康仍标为unsupported；显式
`product-recovery-health-v1`候选也需要自身完整契约及真实原进程/原上下文链。
健康测试/metadata通过不能直接启动timer、确认升级或让unsigned候选可安装。

### 6. 新版底包锁已解析，完整boot来源与启动门槛仍未闭合

需要区分两个入口：旧`tools/rootfs/firmware/rootfs.lock.json`仍unresolved；
新版`firmware/base-system.lock.json`已经resolved，含220包、固定snapshot、
Core UID/GID986、boot/modules哈希和3277MiB尺寸。不能拿旧文件的null值说
新版完全缺包/UID/boot哈希，也不能说已有锁就已完成整机可复现启动。

当前`firmware/kernel/upstream-source.lock.json`明确：
`base_kernel_source_attributed=true`，`boot_source_gate_resolved=false`。
完整kernel/initramfs独立重建和冷启动尚未闭合；它自身也不是刷机清单。
来源复现/匹配ABI核验仍有可离线继续的部分，冷启动/硬件行为属于现场验收。

### 7. 尚未完成的离线证据不能都归为“没设备”

仍可继续推进：最新统一组合的完整回归；实际Root/UID/cgroup/nft产品链；
统计Pending/独立参与授权与身份/业务/outbox的跨域恢复；完整profile资源/
支持矩阵及公开产物审查；kernel/initramfs来源复现。现有15项恢复矩阵明确
不提供全产品快照一致性证明，不能据其通过说统计恢复也完成。

Docker4个官方包和原固定Python镜像层已核SHA；工具仅解压在任务目录，尚未
运行Docker产品链。Debian的独立CLI包及实际原容器夹具接线还需完成。不能
伪造容器标记、把普通systemd测试单位算完整产品链，或进入带BLE的combined
测试来充数。本轮最新main报告没有声称完成这些项。

## 四、CI、工具环境与最终验收

最新main的GitHub Rust run43848116296显示失败；test133233381267和
native-audio133233381609均为0个执行步骤，未开始编译/测试。本轮没有运行
native音频功能测试，仅回读了公共CI状态。仓库历史记录提到计费限制，
但本轮连接器无法读取注释正文独立确认原因，不能把失败标记当代码编译
错误，也不能称CI通过。没有重复刷CI或修改计费/保护。

代理的自动审批服务曾因额度不足中断动作，这是阶段性的执行限制；本次
已恢复并实际完成main复测。当前阻塞不能全归结于工具额度或没装Node。

真实设备的首次启动/升级回退/断电全恢复、第二台、72小时、真实短信电话/
NAS/远程/公网链上等仍无本轮验收。热点按用户安排延期，普通蜂窝流量及
PMIC/关机搁置仍遵守既有边界。没有访问设备、NAS、私有维护目录、生产钥匙，
没有发消息/交易或签名/部署。后续现场工作由负责人在明确授权下执行。

## 五、81.5%与下一步

81.5%来自main的P0～P7固定验收表，不是当前代码量、测试数或本窗口完工率。
原表剩余18.5点为：P0恢复1、P2短信1、P3电话/媒体4.5、P4网络3.5、P5 NAS3、
P6系统/更新恢复2、P7安全/性能/稳定/终验3.5。许多加分点需要完整交付和真实
验收，提交分支或多跑单测不自动加分。169动作台账是另一种口径；任务书的
149未验收动作分为A关联93、B关联29、蓝牙27，不能和18.5点直接互换。

之前“剩两批”只是本窗口后续工作的粗分组，不能解释为整个项目只剩两件事。
当前应逐项报告实际完成/仍缺输入/代码未接/等待审核/现场未验收。

建议顺序：先审核安装修复PR278、测试夹具PR282及来源锁PR271；在主线决定
接纳的A/B代码上形成一个明确完整HEAD，再完成组合构建/回归、Root链与恢复/
发行材料；统计提交驱动按A1冻结契约完成审核；最后执行独立授权的签名/
安装/现场验收。不是把所有PR直接合并后默认视为通过。

本报告仅添加B交付证据，关联PR271；没有改变代码、主线或全局81.5%。原项目
AGPL-3.0-only工具由Codex执行并记录；未虚报工时、作者或测试通过。
