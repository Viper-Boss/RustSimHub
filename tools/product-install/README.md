# 有限产品维护包与执行器

> 窗口8续接：本分支已经接入正式Core/updater、同一个原Snapshot的保留API、
> 仅writer交接、实际执行器Root操作证明和有限恢复健康profile。正式固定路径
> 与runner见[生产集成入口](../product-integration/README.md)。下文“由8接线/
> 尚未导出”描述原模块交付时的职责边界；本次实现与实测状态以新增
> [续接报告](../../docs/deliveries/web4-production-wiring-20261104.md)为准，
> 不将历史harness结果当作本轮正式CLI或设备验收。


本目录交付 WEB4-INSTALL 同源模块，状态为**模块交付，生产入口待集成**。
共享 updater CLI、维护准入和业务锁适配由窗口8接通。本目录没有生产安装
可执行入口，没有生产任意 root 参数。`package.py` 只做离线打包，
`root_matrix.py` 只执行专用测试二进制，不能用它部署。

## 固定合同和调用顺序

签名合同见 `firmware/system/product-install/contract.json` 和
`crates/update/src/product_materials/{package,targets}.rs`。新 kind 为
`product-maintenance`，format1，独立域 `RustSimHub.ProductMaintenance.v1\0`。
旧 AP 八目标表和应用包解析器均不变。包只能提交固定 ID，不提交任意路径、
unit、shell、UID 或健康批准。profile 选择 3/7/10 个固定 Root 材料。

窗口8在 Linux updater 中导出原位置的 `product_materials` 模块即可；模块直接
使用现有 serde/serde_json/sha2/ed25519-dalek/getrandom/fs2/rustix 依赖。
`harness/` 仅用于编译同一份非 cfg(test) 源码，有独立锁文件，不复制实现。

1. 用原安装信任、签名应用和来源事实构造 `ExpectedBindings`，包括实际
   profile/source commit/archive/application manifest/Core/批准角色策略摘要。
   `ProductInstaller` 的公钥参数必须来自原调用者保留的安装信任，并继续检查
   信任文件没有换代；不得从产品包、JSON 或网络输入选 key。
2. 保留原维护准入、实际业务 Store/outbox/writer 锁、固定停机事实和一致备份
   守卫，再实现下述 `LiveInstallationFence`。不能运行中随手复制业务数据库。
   执行器不打开业务 writer、不迁移数据、不生成备份许可。
3. Root 在固定 `/etc/rustsimhub/product-roles.json` 预置独立批准策略，0600、
   uid/gid0、单链接普通文件。format1 的 roles.nkn/ble 各含 uid/gid，必须相等。
   固定名称 `rustsimhub-nkn`/`rustsimhub-ble`，不得与基础角色/音频/IPC别名或
   含额外补充权限。生产只核对 `/etc/{passwd,group,nsswitch.conf}` 和实际
   `/usr/bin/getent passwd/group`；不创建宿主账户、不自动试下一个数字。
   数字981/982只有来自该独立批准策略且实际 NSS 无冲突时才有效。
4. Root 将严格有限材料放在固定
   `/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox`，调用
   `ProductInstaller::prepare(&installed_key, &expected, &live_fence)`。
   `VerifiedProductMaterials` 保留实际 fd/字节，prepare 绑定实际 NSS、旧文件
   基线、备份和真实 flock。新包本身不含安装信任文件或角色策略写目标。
5. 在同一持锁生命周期调用 `plan()`、`apply()`、`inspect()`，异常后下一次合格
   维护调用使用 `reopen()` 再 `recover(false)` 继续或 `recover(true)` 有限回退。
   `plan()` 只输出：离线核验计划的 Root/NSS 基线保持空，执行器的计划在实际
   inspect 后带原基线摘要。所有 JSON 都不能反序列化为存活安装许可。
6. 安装后仍保持关闭。当前执行器/updater/trust/Root合同不在写集，材料阶段后
   切 profile/drop-in，launcher 最后写；全程保留旧守卫，不重启新程序。
   当前合同不支持自行更新执行器、信任或任意跨版本 Root 协议。

## 存活维护守卫接线（必须由8实现）

旧 `MaintenanceSnapshot::revalidate` 绑定原 Root 代次，正常安装本身会改变
该代次。不能在首次变更后无条件忽略它、丢弃并重新观察、释放业务锁，或用
`verified:true`/计划 JSON 替代它。`LiveInstallationFence::revalidate` 必须：

- 在 Prepare、逐文件写入前后、回退、Finalize 都复验原业务、outbox、停机、
  lease/deadline、签名应用/slot/信任事实；原数据锁跨整个操作持续保留。
- 只接受**本次真实执行器**借出的 `OwnedRootChanges` 允许的有限 Root 变更。
  `transaction()` 给固定 manifest SHA；`target_expectations()` 给固定目标 ID、
  原/新摘要、模式、原/写入/恢复 inode 证据；不能从外部构造该视图或许可。
- 限定内部账本为固定 system/product-install 的0700目录、0600锁/active.json，
  以及 transactions/<本次SHA>/{materials,before,journal.json}；包材料原始
  签名字节和备份有界保留，原事务历史不删。只允许实现产生并证明的同父
  `.product-stage-<随机值>` 暂存，不能将整个 `/etc` 或 system 子树放宽。
- runtime 只允许 `/run/rustsimhub/{nkn,recovery}`：Core UID986、独立批准的
  NKN/BLE GID、02750，父目录Root0755。新建及已存在均真核验，不运行通用
  tmpfiles命令。普通用户测试映射为自身身份；真实 Root 专项使用986/1201/1202。
- 每次观察其余 Root 事实仍须与原基线一致。持久账本摘要/路径只是描述证据，
  不能用它重建已丢失的业务或备份守卫；不合格就拒绝或保留 Unknown。

窗口8仍需固定四个生产动作的 CLI 路由、原守卫适配和完整 ARM/PID1/设备
串联验证。不能把本模块测试或 COMPAT 的迁移成功当健康确认、floor推进许可。

## 持久状态与失败边界

journal 为 Prepared/Applying/Applied/RollingBack/RolledBack/Unknown，逐目标
有 Pending/Replacing/Written/Restoring/Restored。替换前先 fsync 备份，再将实际
暂存 inode 写入 journal，才进行 renameat2；目标和目录 fsync 后才记完成。
因此 rename 后 journal 更新前的进程死亡仍可凭 inode+完整摘要恢复，不能只
凭同样字节覆盖另一个后来 inode。prepared journal 已持久化而 active 指针
尚未发布时，prepare 只在完整旧目标/备份/留存签名仍吻合时收养该事务。
未能定位事务或证明旧状态时明确拒绝，不删除 journal 重做。

缺失目标用 NOREPLACE，替换用 EXCHANGE 并核对被换出的旧 inode/字节；
不支持 renameat2 的旧内核/文件系统返回 Unsupported，无不安全降级。
反向恢复只恢复可证明为本事务写入的文件及原模式，原先不存在的只删除自己
证明的 inode。后来写入的数据、身份、floor/attempted/恢复预算/数据库不回刷。
备份/留存签名/锁账本和已创建运行目录、空父目录保留，避免删除后来的 socket
或数据；回退不等于文件系统逐字节恢复原样。失败清理、只读、空间不足、
停机/重载失败或不明基线均返回错误/Unknown，不虚报成功。

服务传输只允许固定 Core/Edge/Media/forwarder 查询、固定 NKN/BLE stop 和
daemon-reload；没有 start/enable、通用命令或网络动作。原四业务服务必须
事先停止。查询1秒、stop/reload5秒，env_clear、输出限4096、子进程回收；
单对象生命周期最多300秒且外部原 lease 仍须有效。模拟服务只证明有限调用，
不证明 PID1、cgroup、真实 NSS 或硬件已经验收。

## 复验命令

Rust1.98，独立 target，`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`，
测试 TMPDIR 置于自己的私有目录；不得让故障测试写满普通宿主文件系统。

```sh
cargo test -p rustsimhub-update --locked --offline --test product_materials_package --test product_materials_rootio --test product_materials_executor -- --test-threads=1
cargo clippy -p rustsimhub-update --locked --offline --test product_materials_package --test product_materials_rootio --test product_materials_executor -- -D warnings
cargo clippy --manifest-path tools/product-install/harness/Cargo.toml --lib --locked --offline -- -D warnings
python3 tools/product-install/root_matrix.py --image LOCAL_IMMUTABLE_IMAGE_ID --executor EXECUTOR_TEST_BINARY --rootio ROOTIO_TEST_BINARY
```

Root runner 只使用已存在的固定镜像 ID，不 pull；本轮 Python3.13 trixie ID
`sha256:bdfc738a8a869292ec1b01f1b8e620db533e09d1fe68fe980a7118b22dae9cec`。
独立容器无网络、只读根、NNP、受限内存/进程，输入只读复制，自有 tmpfs。
执行器只加 CHOWN/FOWNER/FSETID，用真实986/批准GID建立目录。只读专项单独
加 SYS_ADMIN/seccomp=unconfined，仅重挂其匿名 tmpfs；写满专项限1MiB。
每个容器 --rm，绝不挂设备、宿主NSS/数据或可写宿主目录。

普通测试显式略过两个专用故障测试及一个私有子进程入口；Root runner 实际
单独执行前两个。SIGKILL 父测试实际杀死持锁安装子进程，验证 rename 后与
journal 后各自继续/回退，共四次，随后重新获取真实 writer 锁和原数据基线。
这仍不是断电/掉盘测试；真实设备 fsync、旧内核支持、PID1和业务一致备份
接线仍待主线验收。原始日志/临时密钥/合成数据库只保留在 `.local/`。
