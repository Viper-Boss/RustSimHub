# WEB4-INSTALL：有限产品包与安装执行器

阶段一交付，2026-11-04 Asia/Shanghai。用户同时授权本包及独立WEB4-COMPAT；本报告只计本包，不重复计算另包。

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
