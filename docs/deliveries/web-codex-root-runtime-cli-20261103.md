# Root 运行态固定只读CLI交付（第二段8/8）

日期：2026-11-03 Asia/Shanghai。本包两段16项离线开发/隔离验证均已交付，非设备或业务健康验收。

## 来源与堆叠关系

起始main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`，已在开工时成功fetch最新main并核对开放PR/最新分工，独立工作树执行，未动第一窗口checkout/target或文件。第一段组件PR [#197](https://github.com/Viper-Boss/RustSimHub/pull/197) 的最终HEAD为 `ecfb9fe7d8c4b7f940db82cd9ea52b1ebb71b810`，其受测模块实现为 `6736f1c772e4efae33b0ac1bdf6027de4e4e5d51`。

CLI分支 `codex/web-root-runtime-cli-20261103` 继承上述精确组件来源；正式受测CLI、工具及说明HEAD为 **`86744edd70b2d793d364349fa72055e26733f5cf`**。本报告提交只改文档，最终完整PR HEAD记录在PR描述。恢复执行时再次回读#197仍open/未合并、head为ecfb9fe，因此第二PR明确以组件分支为base；不是已经基于合入后的main。组件合入后由主线按实际HEAD接纳CLI，不自行批准/合并。

范围仅 `crates/update/src/main.rs`、新增 `tools/test-update-runtime-observation.py`、`docs/update-runtime-observation.md` 及本报告。组件逻辑全部复用#197，没有复制到CLI；未改health纯关联、Store写入/恢复/confirm、签名/nonce/floor/预算、依赖/锁/工作流、全局分工/看板或另一窗口文件。旧inspect-state及维护入口保持。

## 已交付行为

准确的 `rustsimhub-update inspect-runtime` 在通用可写Store分派前执行；非Root或任何额外路径/PID/单位/时间/JSON参数拒绝。保留既有--build-info行为，仅新增read_only_runtime_inspection_protocol=1。报告生成前完整重验生产组件；保持活的只读守卫直到输出完毕，固定schema单行JSON含换行最多4096字节。失败非0、stdout为空、stderr仅有限错误类别，无部分成功或身份/配置/正文输出。

read_only=true；authorization_verified、health_verified、may_confirm、Core授权boot/业务就绪/data代次及forwarder策略关系始终false。forwarder实际OFF/ON/unknown与仍未知的Core业务开关分开，不启动worker或猜测授权。详见说明及第一段报告。

独立驱动的宿主入口只编排全新本地Docker容器；不在宿主运行固定路径命令，不访问NAS/设备/生产私有目录。只读根、私有PID/文件系统、无网络、七个强制全新tmpfs、只读输入、内存/PID/文件数上限；仅SETUID/SETGID用于非Root控制。镜像不自动拉取，实际image ID固定执行。准备前验证mountinfo/Root/仅lo，数据tmpfs禁执行，仅私有/usr/bin允许本轮合成systemctl。夹具内使用真实更新器生成随机合成密钥/ARM格式包及账本，绝非生产签名/安装。正常和失败退出自动移除容器，外层超时只清理自己的唯一命名容器。

## 实际命令、计数与制品

以下均对应正式受测HEAD86744ed，工具链为已有 `/workspace/toolchains/activate-rustsimhub.sh`，rustc 1.98.0 (88d9e12ae 2026-08-18)。独立CARGO_TARGET_DIR=/tmp/root-runtime-target、CARGO_INCREMENTAL=0、CARGO_PROFILE_DEV_DEBUG=0；复用既有锁与缓存，无新增依赖。各Cargo命令先source上述激活脚本。

| 命令/隔离运行 | 退出码及实际结果 |
|---|---|
| cargo test -p rustsimhub-update --locked --offline | 0；83个运行测试+1个编译拒绝文档测试通过，0失败，4个原专用忽略 |
| cargo test -p rustsimhub-update --locked --offline --all-features | 0；84个运行测试+1个编译拒绝文档测试通过，0失败，4个原专用忽略；两个配置不相加 |
| cargo clippy -p rustsimhub-update --locked --offline --all-targets --all-features -- -D warnings | 0 |
| cargo clippy -p rustsimhub-update --locked --offline --all-targets -- -D warnings | 0 |
| cargo check -p rustsimhub-update --locked --offline | 0 |
| cargo build -p rustsimhub-update --bin rustsimhub-update --locked --offline | 0；实际驱动与旧入口均使用此默认程序 |
| python3 -m py_compile tools/test-update-runtime-observation.py / 驱动--help | 0；提供前置、隔离约束及77=not_run含义 |
| python3 tools/test-update-runtime-observation.py --updater /tmp/root-runtime-target/debug/rustsimhub-update --output /tmp/root-runtime-cli-results.json | 0；31组、49次只读CLI检查；源工作树clean；逐组全过，受保护树不变 |
| 对本HEAD默认lib测试程序，私有Root/read-only/network=none/cap-drop ALL容器执行runtime_observation --test-threads=1 | 0；14/14，无失败/忽略；与第一段同组测试，不重复算独立用例 |
| 原tools/test-update-state-inspection.py --updater /repo/updater（私有Root/chroot容器） | 0；14组通过，9种原无效派发在Store前拒绝；原status恢复控制保持 |
| 原tools/test-maintenance-observation.py --updater /repo/updater（私有Root/chroot容器） | 0；18组通过；真实合成数据UID986/文件锁/WAL/拒绝路径，非PID1 |
| rustfmt --check（本次Root文件）/ git diff --check | 0 |

旧驱动全部来自本HEAD的受版本控制源码及对应firmware材料，复制到本轮私有输入，没有拷贝他人私有脚本。旧驱动容器仅为chroot/合成UID/文件所有权检查加SYS_CHROOT、CHOWN、FOWNER、DAC_OVERRIDE、SETUID、SETGID；仍network=none/只读根，无设备/主机安装，自己的/tmp上限512MiB、内存768MiB。复用实际chroot程序/兼容动态库，不改旧驱动断言。debug --build-info source_archive_sha256为null，未冒充可安装发行归档。

制品绑定：

- 实际默认更新器SHA256：`4af049c79e4f62c42ecacc6665dc2be7d4b6fdbca009bb009033148747cdda35`；旧驱动挂载副本独立回读相同。
- 实际独立驱动SHA256：`37e897a3d43bebdc4cb3e7aa9b3d8dd5342e0db577a6ffe37714456ce33ee3f6`。
- 实际容器镜像ID：`sha256:bdfc738a8a869292ec1b01f1b8e620db533e09d1fe68fe980a7118b22dae9cec`（已存在python:3.13-slim-trixie，无pull）。
- 驱动自己记录tested_source_head=86744edd70b2d793d364349fa72055e26733f5cf，source_worktree_dirty=false；摘要取实际挂入容器的副本，避免后续构建覆盖宿主路径而误绑制品。

## 拒绝/不变矩阵与真实边界

独立驱动31组可由同名case定位：

| 类别 | case及证据 |
|---|---|
| 权限/参数/假健康JSON | nonroot有实际非Root--build-info正控制；extra-arguments、fake-health-json、invalid-before-writable-store。后者持有可恢复state.next且预先删除versions，所有错误均Invalid，未补建/恢复/消费候选 |
| 账本与信任 | missing-state/lock/trust/app、pending/bad-pending/linked-pending、writer-busy、wrong-trust；真实Root文件/共享锁，只读失败 |
| 选中角色 | missing-slot/selected、escape/relative/pending-selected、staged-not-running、attempted-candidate、attempted-wrong-slot；attempted正确来源可到达合成服务/实际不可用cgroup阶段，错误关系先拒绝，不宣称业务已运行 |
| 程序来源 | bad-signature/payload/manifest/oversize-manifest，实际签名/载荷验证拒绝 |
| 子进程故障 | bad-systemd内13种实际查询子进程分支：hang、overflow、exit、bad-utf8、wrong-user、foreign-cgroup、wrong-unit、bad-pid、missing、not-found、pending-reload、malformed、duplicate；预期错误类别及3秒外层界限均断言 |
| 固定总线/环境 | environment-cleared注入SYSTEMD_HOST/MACHINE/BUS_ADDRESS及DBUS系统/会话地址；私有helper检测是否泄漏，实际到达正确Io后置错误而非异常退出，证明固定运输清空环境 |
| 不可用与重复 | systemd-unavailable、runtime-unavailable、repeated-calls；不可用不降级成功，重复不改状态 |
| 原入口 | state-control是真实inspect-state成功、能力false；另跑原14/18组Root回归 |

每次check前后比较全部七个私有tmpfs的文件/链接内容摘要、inode、权限、UID/GID、nlink及mtime/ctime；包括账本/floor/候选/选择、Root工具/信任/合成密钥/包与业务哨兵，49次均不变。普通读取可能更新atime，明确不将atime计作业务写入。只有准备阶段及旧status兼容控制有明确合成状态写入，不当作inspect-runtime的写入。

PID复用（同PID变start_ticks）、采集中换代、boot/时间变化、永久失效、单位/MainPID错配、NNP/所有能力/UID、重复/外来cgroup和关闭worker残留由组件14组覆盖，真实OS或合成边界见第一段逐项表。真实Linux子进程exe/stat/status/NNP、实际exec/退出及阻塞/超量程序已运行；本轮Root14/14再次验证相同组件，未把合成PID字段冒称真实PID复用。实际新CLI在私有Root真实Linux上运行，systemctl属性脚本是合成运输端口，不是实际PID1；所见实际cgroup不可用时正确Io，不伪造成功报告。

## 失败与未运行

新驱动首轮合成systemctl所在tmpfs默认noexec，实际子进程拒绝启动；修正仅本轮私有工具tmpfs为exec，数据保持noexec。补齐七个tmpfs的完整不变比较、实际输入副本摘要和唯一容器超时清理后，正式clean HEAD31组/49次通过。旧Root驱动首轮镜像PATH无chroot，后续128MiB tmpfs不足而失败；只在本轮输入增加真实chroot挂载并将隔离tmpfs上限增到512MiB后重跑14/18组通过。未为取绿改旧脚本、程序断言、生产路径/权限或业务预算。第一段最初编译/夹具失败另已记录。

真实PID1/真实受限服务cgroup、原生ARM签名槽位实际运行全链为not_run：宿主x86与签名ARM格式不同，不能把合成ELF/总线说成实机。Root专用组件不证明业务就绪、实时data代次、Core授权boot或更新发起许可，这些可信来源仍未接通；没有生产confirm、签名安装、启服务、设备/SIM/蜂窝/中继/通话/发送操作。未执行另一窗口的组合回归，不沿用其HEAD或成果。

模块PR197最终ecfb9fe的Actions run41632854242：test/native-audio均0 steps；实际test job125906532136 annotation说明账户付款失败/消费限额不足，作业未开始，不能称CI通过。CLI最终HEAD云端结果将在本PR描述回读记录，未重跑或修改计费/权限。

源码/许可：复用仓库既有AGPL-3.0-only组件、夹具和系统工具，无新第三方代码或依赖。AI辅助编码、验证与审查；沿用现有Git作者配置，未改写历史/贡献或成员授权。全局81.5%、118成员步骤、169动作/20已完成及原权重保持。16/16指本独立任务离线交付；主线审核/合并、实际设备运行与最终验收分别记录。
