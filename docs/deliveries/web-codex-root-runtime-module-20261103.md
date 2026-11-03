# Root 运行态只读来源组件交付（第一段 8/8）

日期：2026-11-03 Asia/Shanghai。起始/最新 main 复核为
`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`；独立工作树
`/workspace/RustSimHub-root-observation`，分支
`codex/web-root-runtime-observation-20261103`。开工 fetch 成功，开放 PR 195 为第一窗口音频工作，无重叠运行态实现。
接口检查点 `6ff0035b55dcfaf980dda55f312e269fcf3738e1`；正式受测实现 HEAD：
`6736f1c772e4efae33b0ac1bdf6027de4e4e5d51`。本报告提交仅文档，不改程序；最终 PR HEAD 由 PR 描述记录，不能把报告的后续提交冒充已测试新代码。

## 已实现与边界

新增 runtime_observation 及 files/linux/units 模块，固定生产入口 Root-only，无路径、PID、单位、总线或时间参数。对象没有 Clone/Deserialize 或 JSON 构造入口，持有实际 ReadOnlyState 共享非阻塞锁；第一次失败永久失效，五秒 Instant 原始期限不续期。report 每次先完整重验。没有 Store::open、恢复、创建、choose、confirm 或确认令牌。

只读上下文复用原账本守卫、签名 verify 和 verify_payload；最小新增只读访问器保留已安装公钥和状态推导。固定 schema6；已安装 epoch 按账本精确匹配，不能把已消耗的全局 floor 当成重新准入阈值。仅暂存的候选不能冒充运行槽；attempted 时严格要求相同候选，可能处于选槽中间态则拒绝。Root-owned/no-follow 源、manifest hash/签名/完整有序载荷集合、大小/24MiB总预算、selected/selected.next 和来源 inode/ctime 均绑定。真实载荷是 `/opt/rustsimhub/versions/<hash>`；账本 versions 的绑定只在 app-select 服务私有命名空间存在，外部观察不错误要求两个目录 inode 相同。

四个固定系统单位映射 core/core/986、edge/edge/985、media/audio-alsa/984、forwarder/forwarder/983。维护组件仅新增固定只读 systemctl 运输接口，复用 Reap，未构造 MaintenanceSnapshot、停业务或取得数据写锁；旧准入判断未改。命令固定 `/usr/bin/systemctl --system show --all`、九个属性、空环境、根工作目录；每次250ms、4096字节、非阻塞管道，超时/异常/超量保持错误并回收本次进程。

生产路径读取真实 Linux boot UUID；Unit Id/User/InvocationID/状态/MainPID/ControlGroup，已有 health::probe_process 的 stat/status/exe，加 exe inode/完整摘要与槽位同一文件、UID四值、NNP、Inh/Prm/Eff/Amb/Bnd均零；核对 v1 systemd/v2 cgroup及有界子组成员。前后重查单位、进程、boot、账本、槽位。重复PID、外来成员、PID退出/exec/代次变化拒绝。8192个proc入口、单status64KiB、cgroup8KiB、32子组/128进程/每组256项为上限；进程读取有已有24MiB程序上限。完整源组合不可得即失败，不返回部分成功。

格式2文件存在不启动worker。报告 forwarder_worker=off/on/unknown 仅指实际进程状态；正常 inactive/dead/PID0/无成员及无UID983残留为off，已运行时仍核对同槽位；失败/过渡态且无残留为unknown。Core期望/授权开关没有可信源，forwarder_policy始终unknown、关系未验证，不从Root gate/配置推断业务授权，不读取凭据/Owner/数据库。off不代表已核实Core的业务开关。无法读取完整proc/cgroup或缺单位也不作OFF证明。

报告只含固定角色、已签名公共摘要/epoch和核对结果，无boot/PID/start_ticks/cgroup现场、配置正文或身份材料。始终 read_only=true、authorization_verified=false、health_verified=false、may_confirm=false；Core授权boot、业务就绪、实时data代次与转发策略关系均false。Linux UUID绝不哈希伪造Core的64hex boot。此组件是只读来源，业务健康与确认授权仍未接通。

## 实际验证（均绑定正式实现 HEAD）

工具链来自环境已有 `/workspace/toolchains/activate-rustsimhub.sh`：rustc 1.98.0 (88d9e12ae 2026-08-18)，既有Cargo.lock/cache，无依赖或锁改动。独立 CARGO_TARGET_DIR=/tmp/root-runtime-target，CARGO_INCREMENTAL=0，CARGO_PROFILE_DEV_DEBUG=0。命令前 source 上述文件。

| 命令 | 退出码与结果 |
|---|---|
| cargo test -p rustsimhub-update --locked --offline | 0；83个运行测试+1个编译拒绝文档测试通过，0失败，4个原专项忽略 |
| cargo test -p rustsimhub-update --locked --offline --all-features | 0；84个运行测试+1个编译拒绝文档测试通过，0失败，4个原专项忽略；与默认不相加 |
| cargo clippy -p rustsimhub-update --locked --offline --all-targets --all-features -- -D warnings | 0 |
| cargo check -p rustsimhub-update --locked --offline | 0 |
| rustfmt（本次新增模块与允许修改文件）/git diff --check | 0 |
| 私有Docker Root、network=none、read-only、cap-drop ALL，运行相同默认测试可执行文件 runtime_observation --test-threads=1 | 0；14/14，无失败/忽略 |

Root容器仅挂本轮复制且校验一致的测试可执行文件，tmpfs /tmp；无服务、设备、主机固定安装路径或网络访问。Root且空bounding set的真实Python子进程完成严格probe正例；外层非Root保留bounding set时如实拒绝。真实exe/stat/status/UID/NNP/cgroup读取，实际同PID exec到sleep及退出、真实子进程阻塞/超量/退出均执行。合成签名包沿用仓库固定合成密钥/AArch64 ELF头夹具；systemd/PID换代和整对象正例用模块私有合成端口。不是实际PID1、不是可运行的ARM程序、不是设备端到端健康通过。

## 可定位拒绝矩阵

所有用例位于 runtime_observation/tests.rs；原 inspection 测试同时回归。

| 类别 | 用例名称关键字及边界 |
|---|---|
| 账本、信任、pending、竞争锁 | missing_wrong_trust_pending_state_and_lock_competition；真实文件/共享锁；执行前后内容、链接、inode、权限相同，无补建 |
| 暂存/已尝试、选择 | attempted_candidate_and_merely_staged_candidate；selected_escape_next；真实合成账本/链接 |
| 签名、载荷、源摘要、集合、特殊文件 | selected_escape_next_wrong_signature_payload_source_and_extra_files；真实签名验证、FIFO不阻塞、超限/符号链接拒绝 |
| 持有期间换链、改程序、extra、next、账本/锁替换 | held_slot_detects；真实文件变更 |
| boot、单位、MainPID、进程代次、采集中换代、失败不恢复 | boot_unit_pid_process_and_mid_collection_change；合成端口；report失败后即使端口恢复仍失败 |
| UID/NNP/所有能力/重复字段/cgroup | malformed_security_cgroup；合成字段；真实Root子进程正例和错误UID/摘要/exe/cgroup/退出另由 real_linux_child 验证 |
| 实际exec | real_exec_replaces_executable；同PID/start_ticks保持但exe改变，旧绑定拒绝 |
| OFF/ON/unknown、旧代worker、子组残留、重复PID、外来成员、format1运行worker | worker_off_on_unknown；合成成员/单位，实际文件版本关联；read_members真实私有子组夹具 |
| 未来/过期、不续期 | future_expired_and_initial_deadline；私有测试对象时间注入，生产无时间参数 |
| 两种boot域 | linux_boot_uuid_is_not_a_core_authorization_boot；真实boot格式读取及合成反例 |
| 命令卡住/超量/异常 | real_subprocess_timeout_overflow_exit；真实Linux子进程有界回收 |
| 坏/重复/多余systemd字段/未加载/错用户/外来cgroup | strict_systemd_parser；合成总线输出，非PID1 |
| 假健康JSON | compile_fail文档用例；不能Deserialize生产观测；报告所有授权/健康位false |

## 失败记录、未运行与后续

首轮编译缺MetadataExt导入和多余导入、首轮夹具权限与Python大于既有24MiB上限导致7个失败、首次Root容器因外层700目录而不能读测试二进制、严格Clippy要求使用?，均已保留并修正各自夹具/代码。没有放宽程序预算、生产断言、Root权限或旧测试。最终结果见上表；不把初次失败算通过。

本PR完成第一段8项离线交付，第二段固定CLI/独立Root拒绝驱动及说明仍由后续堆叠PR交付。真实PID1/单位、原生ARM运行签名槽位与完整真实系统组合not_run；本机x86与签名ARM格式不同，不能伪称真实正例。设备/生产信任/签名安装/Root政策/业务data代次/最终健康和确认均未执行或接通。未改全局台账、第一窗口文件或既有health/Store写入规则。

来源与许可：复用仓库AGPL-3.0-only实现与测试夹具；未复制第三方新源码，未加依赖。AI辅助实现、测试及审查，Git真实身份保持。整体81.5%、118成员步骤、169动作/20完成及既有权重不变；离线交付不等于审核/合并/设备验收。

补充复核：固定show命令显式加入--all，以读取正常OFF单位的空ControlGroup/InvocationID；PID复用负例保持PID不变、只改变start_ticks。正式来源已更新为上述6736f1c完整SHA；默认84总通过/全部特性85总通过（各含1编译拒绝）、4原忽略、严格Clippy/默认check和Root14/14均对该来源重跑通过。第一次复制只读测试制品时权限拒绝已修正本轮副本权限，未改生产权限。此前413f9cfe结果仅为历史来源，不沿用冒充新HEAD。
