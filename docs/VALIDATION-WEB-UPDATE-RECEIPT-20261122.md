# 网页签名应用包受理与维护暂存验证

## 已接通

- 网页先验签预览，用户确认后才提交安装请求。提交绑定已预览的清单摘要，Edge 对提交的文件重新验签并核对摘要。文件只通过设备本地 Core 连接转交。
- Core 使用当前所有者会话和更新权限，提交前及写入完成后重新检查所有者身份。单个受理文件同时保存请求编号、包摘要和完整包，持久化后返回“已受理”。会话凭据留在内存。相同请求核对同一份记录，冲突请求不能替换已受理包。
- 浏览器丢失回应后可查询持久回执。移除已上传文件需要再次确认并匹配具体请求，已暂存或已运行的版本由原更新账本管理。
- 维护端新增固定命令 `rustsimhub-update admit-web-upload <request-id> <manifest-sha256>`。它在既有业务写入围栏下读取固定位置的受理文件，核对库身份和当前所有者代次，再验签、核对版本下限、数据库范围和回退兼容，调用原受限暂存。该命令需要现有维护环境和相同的版本目录挂载。维护端必须从原受理回执携带请求编号与清单摘要；受理槽后来换成另一笔请求时会拒绝暂存。

## 本机验证

- WSL `cargo test --offline --locked -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-update -p rustsimhub-wire --lib --bins --features rustsimhub-edge/airsim-voble -- --test-threads=1`，462 项通过，19 项既有忽略。严格 Clippy 通过。
- Node 更新包、短信转发、控制台和会话四组通过。更新页测试覆盖取消确认、响应丢失后只查询、任务受理与精确移除。
- Edge 通过真实本地连接测试摘要不符和授权拒绝，正例核对 Core 收到的完整字节。Core 持久化测试覆盖错误令牌、内容不一致、重开后查询、幂等、冲突及精确删除。维护测试覆盖内容损坏、符号链接和库身份/所有者代次变更。

固定源码 `235ed7308402edc6a99243d08000a0f9d7b71dfe` 的 5 个 ARM 程序构建及来源回读通过，源码归档 SHA-256 为 `ce731434396a549485c5e38dd647b3f0e797e272f2532e75973a95aeb580382a`。加上沿用的现行音频组件后，6 项应用载荷共 24,809,385 字节，低于 24 MiB 上限。公开进度仓库 `f7caa0eb4393b9634ff1c456370b1637b9694afc` 的 Pages 部署、页面、台账和剩余清单均已回读。

## 暂存前的数据库备份

兼容性通过后，维护命令持有原业务库和转发队列写入锁，复制两个 SQLite 文件（队列存在时），同步并回读比对 SHA-256，然后原子发布最近一份备份。备份与回执保存在应用存储的 Root 私有目录。预检查预留新应用大小和 16 MiB 业务空间。备份失败会停止暂存，不会写入本次候选或提高本次版本下限。成功暂存的 CLI 回执包含 `database_backup_verified: true`。该备份针对同架构、同数据库格式的应用更新，保留原身份和配置文件的维护流程仍按设备现有方式执行。

定向验证使用 `cargo test --offline --locked -p rustsimhub-update --lib admission -- --test-threads=1 --skip same_original_business_and_outbox_flocks_survive_real_signed_installation`，39 项通过、13 项既有忽略。新增用例验证备份回读、滚动替换和拒绝异常路径；既有暂存用例补查备份失败后账本和槽位保持。被排除的旧 Root 安装用例亦在原 3b8dfdc2 下以 Denied 失败，未计入通过。

## 候选撤销

维护端执行 `rustsimhub-update reject-web-trial <request-id> <manifest-sha256>`。两项必须与私有备份回执一致。回执记录原确认槽、业务库格式及候选版本号。命令校验原确认槽签名及载荷、拒绝未完成的账本写入，并在工作服务已停的情况下撤销该候选。下次由原选择器启动确认版，版本下限保持，业务库和备份不被覆盖。旧请求不能撤销其他候选或已经确认的新版。同编号重复查询没有落盘副作用。

WSL `cargo test --offline --locked -p rustsimhub-update --lib web_ -- --test-threads=1` 四项通过，覆盖试启动前后撤销、错绑、回执和数据备份保护。应用槽位测试 16 项及严格 Clippy 通过。本次未在设备上调用命令。

## 维护端单次试运行

`rustsimhub-update run-web-trial <request-id> <manifest-sha256>` 仅由设备 Root 维护入口执行，使用同一请求绑定。先验包并确认没有其他候选，读到活动或未知通话会停止；定时器暂停后等待其服务空闲，再停应用并完成原备份与暂存。启动后通过签名槽位与 Core、Edge、媒体进程的回读确认选中的是本次候选。失败分支用原请求撤销候选并验证原版，随后恢复原来运行的定时器。新版进程正确时返回 `awaiting_health_confirmation`。

WSL `cargo test --offline --locked -p rustsimhub-update --lib web_ -- --test-threads=1` 六项通过，含通话状态解析、模拟调度回退、真实备份和槽位测试。严格 Clippy 通过。

## Setup AP 同格式应用的业务健康确认

现有 Setup AP 启动方式可显式使用 `firmware/system/update-health/56-web-update-health.conf` 以及同目录的 `tmpfiles.conf`。56 放在 Core 服务 drop-in 目录，tmpfiles 放在 `/etc/tmpfiles.d/rustsimhub-update-health.conf`。两项保持 Root 所有、0644。它们不纳入默认镜像，不启用另一个 profile 的健康定时器。当前应用更新不替换 Core 启动器或 AP 资料。

Root 试运行先核对精确配置与运行槽，再通过 Core 的固定私有 socket 获取带挑战与限期的业务证明。确认绑定同一请求、候选和原版摘要、数据格式及接单时的所有者授权证明，最终提交前再次核对。失去最后确认回执时读原账本，不重复确认。无此配置的维护试运行保持待确认。Root 的独立确认接口为 `confirm-web-running-trial <request-id> <manifest-sha256>`，受相同检查约束。

WSL `cargo test -p rustsimhub-update --lib web_` 通过 8 项，`admission::running::tests` 通过 4 项（重叠 1 项），`strict_snapshot` 通过 1 项。Core `cargo test -p rustsimhub-core --lib update_health` 通过 6 项，2 项既有隔离项忽略。两包严格 Clippy 通过。均使用 `--offline --locked`，尚无 ARM/实机全程验证。

## 后台处理及网页结果

可选 Root 材料包含前述健康配置、`rustsimhub-web-update.service`、`rustsimhub-web-update.timer`，以及安装到 `/etc/tmpfiles.d/rustsimhub-web-update.conf` 的 `web-update-tmpfiles.conf`。定时器启用后调用固定的 `process-web-upload`。原持久受理槽继续绑定设备与现有所有者。工作开始前以原请求、包摘要、版本、原确认槽和定时器状态落盘。服务中断后先读该记录和原版本账本，只核对已确认的准确版本或尝试恢复原版。账本未决、仍在通话或发现其他候选时保留记录。刷新后可用原入口查“安装中”“已安装并通过健康检查”或“已恢复原版”；安装中拒绝移除受理包。

使用 WSL `cargo test --offline --locked -p rustsimhub-update --lib web_` 通过 9 项，Core `update_upload` 与 `owner_update_receipt` 各通过 1 项。两包严格 Clippy、Node `tools/test-web-update-package.mjs` 和 systemd 单元语法检查通过。单元语法在临时文件中用本机占位程序解析，未运行设备维护服务或真实 PID1 全链路。

## 待真机复核

仍需用同源 ARM 构建、签名候选和安装后的固定服务验收网页提交、后台安装、健康确认与故障回退。设备最后确认应用 114，整体验收 81.5%，台账 20/169。
