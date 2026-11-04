# WEB4-INSTALL：有限产品包与安装执行器

两阶段模块交付，2026-11-04 Asia/Shanghai。用户同时授权本包及独立WEB4-COMPAT；本报告只计本包，不重复计算另包。以下阶段一保留原提交时边界，阶段二记录完成的执行器及当前剩余接线；状态为**模块交付，生产入口待集成**。

开工main为8f9cfeff1a4b5a804b8b138e890f7386caf5a006，正常合入PR205固定HEAD c3a67120cb91f8c6095579a89764466a101d9433；随后实际fetch并正常快进最新main a2044edfef6aeeced3d2c2eb8743791b34e4ccdc。205已由主线合并（93046b7232e96d10e37380b88fe9c55930f9015c），不是本窗口自行合并。任务文件blob0a704152cfd27264db21a5a9fa0b297a816e009e，当前包独立分支codex/web4-product-install-20261104。

本阶段提交新有限签名合同、固定写集、真实解析/外部信任验签与完整载荷核验、离线编码器和同源测试。实际生产CLI/安装入口尚未接入；执行器、NSS和journal正在下一阶段开发，不能把本提交当完成安装或可部署包。

新kind=product-maintenance、format=1、独立签名域RustSimHub.ProductMaintenance.v1\0。board/arch固定ufi003-v02/aarch64-linux，严格绑定五种profile、40hex来源提交、64hex源码归档/应用manifest/Core程序/独立Root批准角色策略摘要。完整文件有序集合、每程序32MiB/单配置128KiB/总64MiB/manifest32KiB、实际ELF头/摘要及PR205固定模板门禁，严格未知/重复字段。旧AP与应用签名域/解析器原样保留、拒绝新kind；不是扩充旧八目标表。公钥来自调用方已持有的信任，包不提供key或verified许可。

固定写集（全部Root uid/gid0，包内只收固定ID，不收路径/命令/unit/UID）：

| ID | 固定目标 | 模式 | 顺序 |
|---|---|---|---|
| profile_config | /etc/rustsimhub/product-profile.json | 0644 | profile切换 |
| core_dropin | /etc/systemd/system/rustsimhub-core.service.d/50-product-profile.conf | 0644 | profile切换 |
| launcher | /usr/libexec/rustsimhub/rustsimhub-core-launcher | 0555 | 最后 |
| nkn_root | /usr/libexec/rustsimhub/product/nkn-root | 0555 | 材料 |
| nkn_sdk | /usr/libexec/rustsimhub/product/nkn-sdk-engine | 0555 | 材料 |
| nkn_unit | /etc/systemd/system/rustsimhub-nkn-root.service | 0644 | 材料 |
| product_tmpfiles | /etc/tmpfiles.d/rustsimhub-product.conf | 0644 | 材料 |
| ble_host | /usr/libexec/rustsimhub/product/rustsimhub-ble-recovery | 0555 | 材料 |
| ble_unit | /etc/systemd/system/rustsimhub-ble-recovery.service | 0644 | 材料 |
| ble_policy | /etc/dbus-1/system.d/rustsimhub-ble-product.conf | 0644 | 材料 |

legacy-v1只前三个；product-v1/product-email-v1为前三个+四个NKN/tmpfiles；两个recovery profile才加三个BLE目标。各profile复用PR205真实选择，不把五种包无条件写同一集合。目标可有可无，由后续实际旧基线决定备份/回退，不预断言当前设备已安装。updater/当前执行器/trust/key/数据库/floor/候选/预算/开启标记均没有可写ID。新tmpfiles的数字必须再与独立Root批准策略/NSS实际观察核对；不把205的合成981/982当设备批准。

VerifiedProductMaterials持有实际目录和文件描述符、精确签名字节与元数据，不能Clone/Deserialize成脱离守卫的许可；revalidate重查目录集、inode/元数据和完整字节。ProductMaterialPlanV1只有Serialize、私有字段，包含固定来源/应用/角色/目标/权限/属主/步骤与未授权状态，不能由JSON生成安装权限。规划报告没有实际NSS/Root基线时保持false。

离线package.py复用未改的application.verify_package真外部App信任验签及payload检查；核实际git archive提交/源码SHA、PR205程序/材料字节与批准角色策略摘要，再用OpenSSL Ed25519签新独立域。临时私钥仅隔离测试；工具不安装或启服务，不替代持有安装信任/维护准入的生产调用方。

实现后的专用同源integration测试已13/13通过、0失败/忽略，严格专用Clippy通过；Python--help/py_compile与diff检查通过。覆盖所有profile、key/来源/应用/profile/集合、旧签名域、未知角色/目标、字段重复、路径/UID/healthy注入、预算、ELF/模板、真实链接/特殊文件/权限与持活换代，Python实际生成新包后Rust真验签。新包同时实际调用旧AP解析器与旧Rust应用解析器拒绝；无ARM/PID1/设备验收。首次测试fixture编译错误和Python缺cryptography的失败保留私有日志，最终使用已有OpenSSL而非新增依赖，不改旧断言。

本阶段正式实现完整SHA、精确复验命令及结果将于紧接的证据提交/PR描述绑定。执行器阶段未完成的事实不隐藏：实际临时root写入/账户准备、维护锁/备份、持久journal、后写数据保护、有限回退、默认OFF、service控制和中断矩阵须后续真实执行后补报。共享入口由窗口8接通，不能将本模块/签名计划冒称生产安装。

源码沿用仓库AGPL-3.0-only与既有Ed25519/工具链，无新增依赖/锁或旧签名格式改动；AI辅助，保留作者。无设备/NAS/生产密钥/宿主NSS/服务操作，不自行批准/合并、部署或改全局看板。整体81.5%、20/169、加权10.6%及原权重保持。

阶段一正式受测代码HEAD：`b2a82d74906282d1464521eb02db58ff8564354e`。使用本窗口独立target、CARGO_INCREMENTAL=0、CARGO_PROFILE_DEV_DEBUG=0和私有TMPDIR：`cargo test -p rustsimhub-update --locked --offline --test product_materials_package` 退出0、13/13、无失败/忽略；`cargo clippy -p rustsimhub-update --locked --offline --test product_materials_package -- -D warnings` 退出0。两者在该提交后已重跑。验证为实际Ed25519/OpenSSL/文件IO与旧解析器拒绝，合成ELF/临时信任明确不能当真实ARM/生产密钥。阶段一证据补交仅文档，最终完整HEAD由PR回读记录。

## 阶段二：实际安装、持久恢复和有限回退

阶段一 PR [#209](https://github.com/Viper-Boss/RustSimHub/pull/209)，完整HEAD `42dca70ff82b405b787616def708903bb126bad5`。阶段二独立叠加分支 `codex/web4-product-install-executor-20261104`，正式受测源码完整HEAD **`d73eb671cd68400820b71aced83e5ab3ebc34377`**，其 git archive SHA256 **`e6dc60435e326b1d13b7a84704ef5b73a312f9ee8001e84d28803c172972dc67`**。本节是随后仅报告提交，受测源与测试没有修改；最终PR完整HEAD另在PR描述及GitHub回读绑定。

新增同源 rootio/nss/services/executor 实际实现。生产根固定为 `/`，输入固定 system/product-inbox，事务固定 system/product-install；没有生产任意目录入口。isolated root、合成NSS和模拟service只在cfg(test)存在。Root fd逐父目录nofollow、普通文件单链接/属主/权限/预算、同父随机EXCL暂存/读回/fsync/目录同步、实际renameat2 NOREPLACE/EXCHANGE及被换出旧inode复验。不能安全证明或内核不支持就拒绝，无跟链接/通用覆盖降级。

NSS独立Root批准策略0600，实际local+有界getent全量核对固定名称、数字别名、补充组、audio29/IPC987关系、nsswitch来源。生产只核对不分配账户；测试only真实修改自己的临时passwd/group，冲突不选下一个UID。运行目录只允许Core986/独立批准NKN或BLE组/02750的两个固定路径，生产不运行任意tmpfiles命令。普通用户测试明确映射为自身，Root容器实际使用986/1201/1202，不能把后者当设备批准。

ProductInstaller持有实际材料fd/NSS观察、真实事务flock及调用方借出的存活维护/DataFence。prepare写前绑定实际旧字节/模式/元数据，持久备份和原始签名源；安装不另开业务writer。journal Prepared/Applying/Applied/RollingBack/RolledBack/Unknown，逐目标真实暂存inode在rename前先持久化，rename后及journal前死亡可凭inode+完整摘要恢复。prepared journal发布活动指针前中断，只在原目标、备份和留存签名均重验通过时重新收养；基线变化拒绝。

apply先材料、再profile/drop-in、最后launcher；当前执行器/updater/trust/Root合同无写ID，原守卫持续保留，不重启新程序。inspect为实际文件重查；reopen即使原inbox消失也重新验留存原始签名字节。recover显式继续或逆序有限回退，只有本事务证明的旧目标才恢复原字节/模式，原先缺失的仅移除自己写入的inode。后来不同inode/字节不覆盖，失败清理/重载/变代保持错误或Unknown，不假报原样。

保留事务材料、备份、账本、空父目录与运行目录，不删除后来socket或数据；回退不是整盘还原。业务DB、floor/attempted/预算/身份、开启标记没有写入口。固定service只查询四业务已停止状态、停止两个可选worker、daemon-reload并复核；无start/enable/网络动作。查询1秒，stop/reload5秒，有限输出/空环境/回收子进程；对象最多300秒且外部原lease仍须有效。模拟service只证明这些调用，不冒称PID1/cgroup验收。

ProductMaterialPlanV1现在公开导出：离线plan未观察Root/NSS时摘要为空；执行器plan先inspect再附本事务实际Root/NSS基线摘要。它仍仅Serialize、私有构造、authorization_verified=false，不能把JSON、未知字段或“healthy”变成存活许可。报告固定health_verified/may_confirm/shared_cli_integrated=false。

## 冻结源码后的最终验证

Rust1.98，独立target、`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`，自有私有TMPDIR，所有Cargo均 `--locked --offline`。以下均在d73完整HEAD提交后重跑退出0，复验前后工作树源码clean。原始失败/最终日志仅私有 `.local/logs/`，没有提交DB、密钥或硬件数据。

| 命令/边界 | 实际结果 |
|---|---|
| `cargo test -p rustsimhub-update --test product_materials_package --test product_materials_rootio --test product_materials_executor -- --test-threads=1` | 包13、RootIO/NSS8、执行器10，共31通过；两个专用tmpfs测试与一个私有child入口显式ignored，未算普通通过 |
| 同三target `cargo clippy -- -D warnings` | 退出0 |
| `cargo clippy --manifest-path tools/product-install/harness/Cargo.toml --lib -- -D warnings` | 退出0；真正非cfg(test)同源生产模块编译，不是只编译测试替身 |
| `cargo test -p rustsimhub-update --lib` | 原库58通过，0失败；2个原专用Root ignored未计通过 |
| `python3 tools/product-install/root_matrix.py --image LOCAL_IMMUTABLE_IMAGE_ID --executor EXECUTOR_TEST_BINARY --rootio ROOTIO_TEST_BINARY` | 退出0：真实Root执行器10、RootIO/NSS8、专用ENOSPC1、专用只读1；与普通测试重叠部分不重复累加为新覆盖 |
| 两个新Python工具py_compile，root_matrix.py --help，git diff --check | 退出0 |

执行器10组含五种profile实际安装/权限/幂等/回退、不同签名版本升级再回到上一版、缺失目标、inbox消失后留存重开、原备份/服务/lease/NSS/journal变更、后来Root写入保留、默认OFF与拒绝错信任/开启标记。实际业务SQLite夹具先创建schema6并关闭连接，再持有真实writer flock；安装全过程检查原业务及floor/预算/NSS字节不变。普通故障hook在替换前后、journal后、重载前后、恢复前后模拟错误；另外真正启动持锁安装子进程，rename后/journal后分别SIGKILL后继续或回退，共4次，原错误处理及Drop不能运行，随后实际重取writer锁和原数据基线。不是物理断电测试。

Root runner固定现有Python3.13-slim-trixie镜像ID `sha256:bdfc738a8a869292ec1b01f1b8e620db533e09d1fe68fe980a7118b22dae9cec`（镜像repo digest `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`），不pull。私有无网/NNP/只读rootfs/只读输入/新tmpfs，Root执行器仅加CHOWN/FOWNER/FSETID；只读专项独立加SYS_ADMIN并仅重挂匿名tmpfs，ENOSPC限定1MiB。实际主机NSS、设备、服务和生产DB从未挂入或写入；容器仅清理自己的唯一名称。

早期失败如实保留：测试模块重复引用/未使用符号导致编译和Clippy失败，已按同源模块边界修正；容器fixture父目录缺失/源材料0700与Python绝对路径导致失败，改为只读复制公共模板、正确父路径和绝对测试解释器，没有增加DAC越权能力或放宽断言。另有一次中断恢复Io未捕获底层errno，不能确定其根因；也实际遇到一次工作区目录创建ENOSPC。迁移自己可重建测试输入后最终固定HEAD顺序复验及Root专项全部通过。该环境故障与专用1MiB真实ENOSPC覆盖分开记录。

## 十二动作与窗口8接续

| 任务书动作 | 本模块已实现/验证 | 未冒称完成的外部验收 |
|---|---|---|
| 1～2 | PR205有限写集、独立签名合同/编码器/外部验签、旧解析器真实拒绝 | 生产签名制品与设备安装批准 |
| 3～4 | 实际NSS及固定目录核验、测试账户准备、真实安全文件执行器 | 实际设备UID批准/配置与旧内核能力 |
| 5～7 | 存活fence接口、实际旧目标备份、持久journal/重开/幂等/有证据回退、SIGKILL恢复 | 原维护一致备份/DataFence受限Root代次适配 |
| 8～9 | 默认OFF、有限有界service传输、材料/profile/launcher最后顺序、不自更新信任/执行器 | 实际PID1/cgroup/ARM自更新整链 |
| 10～11 | 真签名/临时Root安装升级与故障反例、真实Root属主、只读/写满/竞态/后写保护 | 实机断电/存储/设备验收 |
| 12 | 两个可审阶段、完整受测SHA/命令、固定写集、README最小接线清单 | 主线审核合并和生产入口接通 |

窗口8精确清单见 [tools/product-install/README.md](../../tools/product-install/README.md)。核心缺口不是再读一份JSON：旧MaintenanceSnapshot绑定整个原Root代次，安装drop-in必使它失效，8须保留原业务/outbox/writer锁和实际停机/备份条件，实现仅允许本执行器proven Root写集的受限revalidate。`OwnedRootChanges`给本事务、固定目标新旧摘要/模式及写入/恢复inode；不能丢锁重新observe或忽略其他Root变化。8再接固定prepare/apply/inspect/recover动作、安装信任/槽位事实、真实ARM/PID1整链。共享lib/main/admission及旧八目标表本包均未改，当前未接生产CLI。

本包独立模块和隔离验证交付完成，状态仍**模块交付，生产入口待集成**；不称生产安装/健康确认/schema9准入完成。独立COMPAT阶段PR210/212由另包报告，不重复计本包。未部署、未使用真实密钥、未改宿主账户服务、未自行合并或改全局台账，整体81.5%、20/169、加权10.6%保持。

PR209云端CI实际run41733471625为failure，两个job steps=[]、runner_id0；check注释明确账号payments/spending limit导致job未启动，不能算云端测试通过或代码测试运行失败。阶段二创建后按实际PR完整HEAD另行回读并记PR描述。
