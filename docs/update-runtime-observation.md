# 更新器只读 Linux 运行态来源（协议1）

`rustsimhub-update inspect-runtime` 只有这个准确形式，不接受root、trust、PID、单位、总线、时间或JSON。Root权限、当前正式安装来源和系统元数据都必须可验证；额外参数在任何可写Store、创建或恢复之前拒绝。`--build-info` 的 `read_only_runtime_inspection_protocol: 1` 仅说明支持本只读协议，不改变安装/Root契约或默认功能。

此组件不自动确认更新，不证明Core业务就绪或更新发起授权；报告与关联结果均不能交给confirm当许可。实现提供实际只读来源与持有期重验，后续生产者、授权及健康政策由主线另行接纳。

## 固定输入与生命周期

- 账本 `/var/lib/rustsimhub/rustsimhub-v1/system/app`，已安装公钥 `/usr/share/rustsimhub/release.public`；持有原有 `.update.lock` 共享非阻塞锁，锁/来源替换、缺失和任何 `state.next` 均失败，不恢复或补建。
- 选择 `/run/rustsimhub-app/selected` 只能是Root拥有的绝对链接，精确指向 `/opt/rustsimhub/versions/<账本manifest摘要>`；拒绝selected.next、越界、签名/载荷/大小/集合错误和来源变化。候选仅暂存时仍应运行confirmed，已attempted时只接纳该候选；中间态保守失败，不消费或修账本。已安装包精确epoch与账本绑定，不把已消耗floor重新作为stage门槛。
- Core/Edge/Media/可选worker分别固定core.service/986、edge.service/985、media.service/984、forwarder.service/983及对应服务账户。读取真实系统单位的Id、MainPID、状态、User、ControlGroup、InvocationID、LoadState与NeedDaemonReload，结合真实proc start_ticks/四UID/NNP/五组能力/exe同一inode及已签名摘要/cgroup成员重验；不能用is-active代替角色证明。所有单位须可读取并已加载，元数据不完整时失败，不启动缺失服务。
- `/usr/bin/systemctl --system show --all --no-pager` 只查询固定属性和单位，子进程环境全部清除，stdin/stderr丢弃、工作目录根、stdout至多4096字节，每次250ms。错误、坏/重复/额外字段、超量及超时不重试取绿，杀并回收本次查询子进程，不动业务进程。
- 进程程序受既有24MiB上限，完整槽位总预算24MiB；status64KiB、cgroup8KiB、proc入口8192、32个cgroup目录、128个成员、每组256个目录项。v2或v1 systemd归属明确；不完整、外来/重复、额外残留子组成员均拒绝。UID983扫描只读status，不读cmdline/environ、Owner材料或业务内容；无权读取也不当作“没有worker”。
- 五秒有效期从系统Instant开始，不接受外部“现在”；成功revalidate不续期。报告前核对整个来源组合，任一失败使对象永久失效。另读真实Linux boot UUID前后比较；它不是Core的64hex授权boot，不会变换或哈希成授权boot。只读多来源扫描是有界时间内的前后核对，不是暂停内核/业务取得原子全系统快照。

选槽服务的 `/opt/.../versions` 到账本versions绑定存在于它自己的私有命名空间；本组件直接核对真实只读载荷，不能错误要求主机上两个目录已全局绑定。不获取停机准入的数据写锁，不用停止Core换取观察。

## 输出与含义

成功一次输出协议1单行JSON，固定schema、总长含换行不超过4096字节，无部分成功JSON。失败非0，stdout为空，仅输出有限错误类别。报告不输出现场boot、PID/start_ticks、cgroup路径、配置/身份材料或秘密。

| 字段 | 固定含义 |
|---|---|
| format / scope / read_only | 1 / linux-runtime-sources / true |
| authorization_verified / health_verified / may_confirm | 始终false |
| application_integrity_verified / runtime_selection_verified / process_bindings_verified / linux_boot_stable | 成功时完整核对本轮槽位/进程/boot来源；不扩大成业务健康 |
| selection / candidate_present | confirmed或attempted_candidate；候选存在只描述账本 |
| epoch / manifest_sha256 / source_sha256 | 已签名并匹配账本的公共版本元数据 |
| processes | 固定角色及signed_executable_bound，不含现场PID等 |
| forwarder_worker | off/on/unknown只描述实际工作进程状态；格式2文件存在不等于on |
| forwarder_policy / forwarder_policy_relation_verified | 始终unknown / false；Core期望/授权开关与当前worker关系没有可信源 |
| core_authorization_boot_verified / business_ready_verified / data_generation_verified | 始终false，未接通可信业务来源 |
| max_age_ms | 5000，重验不能续期 |

正常OFF允许inactive/dead/PID0与空cgroup/无UID983进程，不要求启动worker。ON必须同槽位/同单位/同UID且无多余旧代worker；失败/过渡但无进程时unknown。有残留、来源不完整或无法核对时拒绝。off不证明Core开关已授权关闭；本组件不读enabled/settings/凭据来猜测、不启用/重配/递送。不要把此报告转换成现有health::correlate所需的伪可信业务fence。

## 独立离线验证

使用自己的工作树和target目录，保持Cargo.lock和默认开关。可在自己的开发环境编译：

```sh
CARGO_TARGET_DIR="$PWD/.local/root-runtime-target" CARGO_INCREMENTAL=0 cargo test -p rustsimhub-update --locked
CARGO_TARGET_DIR="$PWD/.local/root-runtime-target" CARGO_INCREMENTAL=0 cargo test -p rustsimhub-update --locked --all-features
CARGO_TARGET_DIR="$PWD/.local/root-runtime-target" CARGO_INCREMENTAL=0 cargo clippy -p rustsimhub-update --locked --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR="$PWD/.local/root-runtime-target" CARGO_INCREMENTAL=0 cargo build -p rustsimhub-update --locked
python3 tools/test-update-runtime-observation.py --help
python3 tools/test-update-runtime-observation.py --updater "$PWD/.local/root-runtime-target/debug/rustsimhub-update" --output "$PWD/.local/root-runtime-results.json"
```

驱动只在本地Docker守护进程新建的私有容器运行真实固定路径CLI；不会在宿主机调用inspect-runtime或访问实际安装。默认要求本地已有兼容`python:3.13-slim-trixie`镜像，无自动pull；读回实际image ID并固定该ID执行。容器无网络、只读根、私有PID/文件系统、资源限制，仅加SETUID/SETGID用于非Root拒绝控制，其余能力移除。七个全新tmpfs先通过mountinfo和私有网络验证，才在其中准备/清理合成材料；仅`/usr/bin`允许执行本轮合成systemctl，数据tmpfs不可执行。宿主机仅复制本轮程序/驱动到自己的临时目录并只读挂载，所有正常/失败退出自动移除容器，外层超时只清理自己的唯一命名容器。

夹具准备使用真实更新器生成本轮随机合成签名包/账本，完全不接触生产种子。观察前后比较七个tmpfs的内容摘要、inode、权限、UID/GID、链接、mtime/ctime；不比较普通读取可能更新的atime。包含合成Root工具/材料、floor/候选/选择、信任、签名包、种子及业务哨兵；不输出这些内容。无效参数同时保留state.next并删除versions，证明未进入可写Store去补建/恢复。

输出提供程序/驱动SHA256、实际Git HEAD和工作树是否有未提交内容、image ID，以及逐组结果。0表示离线矩阵通过；77仅表示本地Docker/镜像前置不可用且未运行，已执行断言失败返回1，不降级跳过。

当前31组包含真实Root CLI、非Root正反控制、旧inspect-state、只读不变和systemctl实际子进程失败/环境清空。systemctl属性由本轮合成脚本提供，明确不是PID1。进程换代/exec/UID/NNP/能力/cgroup/boot/年龄/永久失效分别由模块测试覆盖并标明真实OS或合成边界。x86宿主上AArch64签名格式的程序不可直接运行，完整原生ARM槽位+实际PID1成功、真实cgroup服务组合标not_run；不篡改ELF/旧断言冒充成功。不运行会碰生产固定路径的驱动，不复用他人私有目录，不签名安装、启服务或访问设备。

两份堆叠PR：组件PR #197先到main；CLI PR以组件分支为base（组件尚未合并），组件合并后由主线按实际HEAD接纳CLI。两份交付报告分别记录受测/最终来源、计数和缺口；不把另一窗口音频/运行器或设备验收算入此包。
