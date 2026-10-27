# U2 · 转发数据的只读版本检查

2026-10-27。UPD-01阶段二的持久数据检查已实现；Root/候选/回选组合准入仍缺，不把整个UPD-01计完成。未连接NAS/棒子、发送通知、安装材料或操作分区。

## 实际入口

生产forwarder新增`--inspect-outbox <绝对目录>`，在Tokio、凭据、Root动作和worker初始化之前处理。只能检查当前有效UID拥有的已有0700目录、0600单链接数据库及已有空writer锁；不会创建缺失目录、锁或数据库。使用与实际Queue相同的`.writer.lock`，非阻塞独占；检查对象被Core使用时拒绝。检查守卫在序列化回执期间保留锁及SQLite只读事务，释放后报告只是历史观察，不能直接当选择应用的授权。

先拒绝任何journal/WAL/SHM（包括空文件或悬空链接），检查SQLite文件头为DELETE模式，再用READ_ONLY/NOFOLLOW连接和query_only/trusted_schema关闭设置读取application_id、user_version及quick_check。先查文件头是必要的：SQLite只读连接仍可能在可写目录创建WAL辅助文件。检查前后复核目录与数据库节点，禁止以恢复、切换日志模式、打开Queue或迁移来“修好”检查对象。

数据库上限32MiB，SQLite零busy等待；进度回调限制1000万VM操作及约3秒查询预算。文件系统IO不属于VM预算，有限维护调用者仍须设置进程墙钟超时。rusqlite只新增既有版本的hooks feature，Cargo.lock及依赖版本不变。

返回格式1只含数据格式、application_id及当前运行时比较结果：read_write、migration_required、foreign_application、unsupported_format。旧格式1/2是需迁移；未知数据及未知运行时契约不默认升级为当前能力。输出authorization_verified=false，不返回Owner绑定、短信、目标、密钥或请求。SQLite一致性和版本可比较不等于业务表结构完整、Owner有效或允许回退，不能据此迁移/递送/改变开关。

## 独立证据

最终冻结工作归档740文件，SHA-256 `de9362c640c890aa41182aaaba152a5b18f985e9a570fd4899eae4082b89dd86`。只读源码/依赖、UID65534、空能力、NoNewPrivileges、私有网络/设备，显式清理Core/forwarder后构建。证据位于`.private/upd-inspection-u2-20261027-second/`。

- Core157、forwarder92、Wire18，共267项普通测试通过；专用ENOSPC仍单独ignored，不混入本次执行数量。三包严格Clippy、workspace fmt及Core/worker生产构建通过。
- 9项真实SQLite检查测试覆盖实际Queue创建的数据、格式1/2不迁移、in_flight/Unknown/confirmed正文逐字节保持、两方向真实writer锁、缺文件不初始化、全部辅助文件/悬空链接、外来/未来格式、权限/链接/超限、损坏/WAL模式，以及无限递归SQL被实际操作预算中断。另有1项未知运行时契约拒绝测试。
- 真正生产CLI的19个进程用例：6种版本/应用身份、缺锁/缺库、另一进程持锁、三种辅助文件、悬空journal、软/硬链接、FIFO、损坏、权限，以及实际SQLite子进程在未提交写入后os._exit留下的journal。每个调用5秒截止，前后对比全部文件字节摘要、节点、权限、大小和mtime，回执与独立期望一致，无私密合成正文输出。
- 两个实际生产CLI的build-info同源/无dev-fixtures/契约核对保持通过。第一版266项和19个进程用例也通过；最终新增VM预算后重新完整执行，按最终267项计，不合计两轮。
- CI新增在真实生产worker构建后执行同一CLI脚本。本次源码的精确提交CI结果另行记录；不借用旧提交成功冒充当前CI。

## 固定提交的CI与ARM补验

代码HEAD `02204a8443bcc5885eb87f7c7f6f169dffe761ce` 的[push CI40447155396](https://github.com/Viper-Boss/RustSimHub/actions/runs/40447155396)和[PR CI40447159015](https://github.com/Viper-Boss/RustSimHub/actions/runs/40447159015)均completed/success；两个运行各自test/native-audio均成功。独立读取两个test job的实际日志，均包含9项inspection测试、查询预算中断和19个生产CLI用例通过，不只依据总状态。

从该提交的Git精确字节重新冻结741文件，归档SHA-256 `a2a1c6bc4470975671dfeff9f8091e2a709d92e0777b1e6edfc270faba76a0c2`；全新空target、生产feature构建11个ARM64程序，readelf/GLIBC界限通过。实际Core/worker经QEMU输出同源当前契约，实际ARM worker经QEMU执行同一19进程检查全部通过；8文件Root维护包同源/签名/外部公钥验签通过。材料仅保存在`.private/upd-inspection-production-*20261027-first*`，不安装信任、不部署到设备。首个临时shell因Windows换行在编译前退出，修正LF后首次真正构建成功，未覆盖失败为成功。

这些结果只支持U2代码及材料，不证明设备启动、真实回选或Root联合准入。补录证据的后续文档提交另有自己的CI状态。

## 仍待完成的联合准入

1. 用可信的实际Root安装配置和选定/候选/回选程序做联合比较；历史缺失能力需维护或拒绝，不能猜旧outbox可读版本。
2. 在有限维护准入中持有完整停止/身份/签名/数据观察约束；当前CLI报告本身不授予更新权限。
3. UPD-02签名材料接线及实际格式1/2/3回退、冷启动分别保留原验收；本阶段生产ARM已补验，后续联合准入变更仍需对应新构建。

四位成员的文件边界不变，PMIC/关机继续搁置。原P0–P7为80%，扩展台账14/169、加权7.3%保持。
