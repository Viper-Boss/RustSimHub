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

## ARM 候选与只读设备预检

源码固定为 `220cec9f0236449d7a6255572f7cf1719a391a37`，归档 SHA-256 `ace719f43077ad3e4b0b31037ae3cb1562453ca965c77f74f8859c81bc9c8377`。执行 `cargo +1.98.0 build --release --locked --offline --no-default-features --features rustsimhub-edge/airsim-voble --target aarch64-unknown-linux-gnu -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-forwarder -p rustsimhub-ctl -p rustsimhub-update`。五个程序经 ELF 和 QEMU `--build-info` 回读匹配该来源。

四个新应用程序与已验签的旧音频程序和许可证组成六项应用载荷，合计 24,820,505 / 25,165,824 字节。新 ARM 更新器在 QEMU 中接受原 114 签名包与一次性本机测试候选；拒绝错钥、损坏载荷、覆盖原包或多余的维护参数。五份固定配置材料取自同一源码归档，连同 Root 更新器共六个固定维护目标。预备两份未签名草案，测试钥匙不用于设备。

设备只读确认为 114，版本下限 114，应用服务运行，无待启动候选，新 Root 服务尚无安装。尚未使用设备信任钥匙签名或执行安装。

## 实机容量前置修正

在 114 运行时只读核对，业务入库目录和应用账本同在一个分区，可用 17,468 KiB；应用槽所在分区可用 994,292 KiB。迁移前构建仅保留为对照。新增固定 `/opt/rustsimhub/web-update` 目录，将暂存包和滚动数据库备份分别放在 Core 和 Root 私有子目录。Core 只在启用对应固定挂载的应用上接受上传，预留额外磁盘空间；维护程序核对两个目录的属主、权限及与应用槽相同的文件系统。账本、业务库与小型确认文件继续留在原数据分区。

WSL 更新器 10 项、Core 状态及旧受理回归各 1 项与严格 Clippy 通过。临时树使用实际 systemd-tmpfiles 核对目录属主和权限，服务与定时器语法通过。设备数据和配置未写入。

## 容量修正版 ARM 候选

固定源码 `aa714f0417c606582fd7516b0ce602c96c4885bb`，归档 SHA-256 `6c3c14e261223a992544f79c01fd52c23d55d40042a40950dd316f57c1435feb`，沿用上述五程序构建并逐一回读。六项载荷合计 24,820,809 字节，低于 24 MiB 上限。新 ARM 更新器用一次性测试钥匙对新包走完打包与验签，原 114 包仍能验签。拒绝错钥、损坏载荷、覆盖和多余维护参数。六项 Root 文件及两份未签名草案已核对。未进行新签名、上传或安装。

## 待真机复核

仍需安装入口版和固定维护材料后，在真实网页验证提交、后台安装与健康确认。故障回退保持本机证据，单列真机验收。整体验收 81.5%，台账 20/169。

## 入口版安装与恢复准备

本机复核两份未签名草案及六项固定维护文件。以隔离目录验证未备份时拒绝、正常安装和恢复、部分安装后用原始备份恢复，以及拒绝原目标替换和链接。用 Python 编译检查安装脚本，并用 `sh -n` 检查定时恢复脚本。真机应先核对确认版、空闲通话和存储，再完成备份回读及数据库检查，随后暂存和试运行。自动审批拦截设备签名密钥读取，本轮未签名、未上传、未安装。网页更新完整链路仍待真机验证。

再次只读运行更新器 `status`、逐一查看应用服务及定时器、核对调制解调器通话状态与 `df -Pk`。确认 114、无待运行候选、七项应用服务及两项业务定时器正常、无活动通话。业务分区余 17,468 KiB，应用分区余 994,292 KiB。未重新申请写入设备。

## 115 真机入口版

收到针对 115/116 的明确授权后，以 `wsl.exe -d Ubuntu -u root -- python3 .local/prepare-signed-web-aa714f04.py --sign` 对固定源码 aa714f04 签名并使用 ARM 更新器独立验签。115 和 116 的清单摘要分别为 `ae8dba6be104be3bd990230a582754a134119c033159f941383fe662fbfecca7`、`6a001e7edb4ab0498160f7438218ce35c2caba53bb4c65fe55a4dc9bcbdd0690`。

使用 `python .local/install-web-bootstrap-aa714f04.py --install-115` 核对设备空闲，将新备份下载回读并检查 SQLite 完整性。实际设备更新器验签通过。新试运行保持原短信、业务表、身份配置和 AirSIM 配对，原 HTTPS 地址返回固定源码内的更新页面资源，随后确认 115、保留 114 应用槽、恢复原两项业务定时器并启用网页更新定时器。真机未重启或刷分区。116 仍需网页提交和健康回执。转交浏览器时工具连接超时。

### 115 网页原生调用修复

实机网页选中 116 后提示暂时无法验签。只读检查确认 115 稳定运行、后台未见受理包或运行记录。带固定 TLS 验证的空包请求分别验证未登录 401、伪造会话 403。独立 Chrome 测试观察到把原生 `fetch` 当作组件方法调用会抛出 `Illegal invocation`，绑定到 Window 后成功。预览、提交和确认统一通过 Window 调用；`node tools/test-web-update-package.mjs` 覆盖默认浏览器函数接收者并通过。24 MB 全量本机 Chromium 测试因自动审批额度未执行。设备仍在 115，不再使用网页尝试旧 116。

审批恢复后运行 `python .local/check-native-update-browser-20261123.py`。真实 Chrome 使用独立临时配置，回放 115 原网页模块和修复模块。前者在发出请求前失败，后者完整发送 24,821,842 字节，模拟接口逐次回读匹配包摘要，预览与提交回执均通过。模拟的确认不代替真机安装。另集成登录页和首页的 NKN 公开活跃安装统计自动读取，`node tools/test-web-usage-stats.mjs` 验证页面绑定、重复请求、失败保留上次统计和手动重新读取。

修正版冻结源码 `0b3701086037768732687194631517ef7d732312`，归档摘要 `75a70eef0e0f26a3b7782b76babc869e10f3f25a39241f8bf94ad4381bfae45c`。使用 WSL ARM 交叉构建、QEMU 五项来源回读和一次性测试钥匙完成包格式验证，115 安装的更新器对应 ARM 产物也验过正常包。两份草案核对继续复用 115 的媒体文件，源归档一致。载荷 24,824,905 / 25,165,824 字节。准备的 117 安装流程只使用 115 已启用的维护程序与配置。尚未使用设备私钥签名或改动设备，下一步是先确认 117，再网页升级至 118。

## 真机 117 与网页 118

获得明确授权后为同源 117/118 签名并通过 ARM 验签。117 安装前完成业务备份回读、SQLite 完整性、原业务表与 AirSIM 配对核对。确认 117 后保留 115 槽及原维护配置。真实 Chrome 登录前自动显示 NKN 活跃安装统计，三组窗口各为 1。

真实 Chrome 选中同一签名 118 并完成验签，用户确认网页提交受理。只读检查受理包头的清单、完整包摘要和后台请求匹配。后台记录 `restored`，应用账本确认版及下限均为 117，无候选或 118 版本目录，没有产生本次业务备份。核心、媒体、网页及相关定时器处于正常状态。日志显示目标单元停止时，Core 和选择器并未退出，更新器随即恢复目标，未越过准入门禁。原请求未删除、未重提。

## 两份备用、顺序回退与手动清理

网页试运行现在显式停止并核对选择器、Core、Edge、媒体及已安装的相关服务。拒绝时记录失败阶段。应用确认后，版本账本保留新到旧的两份已确认备用。验证不通过的槽会被跳过，启动故障通过固定恢复单元尝试下一份。恢复只能消费当前选中槽，次数有上限，旧受理下限不降低。

后台在无待运行候选且选中槽与账本一致时清理未引用的旧签名应用槽，保留未知文件及较新的未提交材料。删除从版本账本撤销引用后才移除磁盘目录，保留中断可检查的目录标记。升级页展示当前应用和两份备用，最近登录的所有者确认后，Core 只接受固定版本及摘要，后台只处理与当前选中应用匹配的短时请求。启动/退出中不得删除当前版或候选。工厂镜像、业务数据库和分区不在清理路径。

本机验证：WSL 更新器 114 项、Core 更新 3 项、协议 29 项，严格 Clippy 通过。旧 `same_original_business_and_outbox_flocks_survive_real_signed_installation` 与先前基线一样报 Denied，后续定向运行排除此项，另有 17 项旧专用测试忽略。隔离 Shell 和真实 systemd 单元共 5 项，Node 网页更新及统计回归通过。实际棒子暂未更换恢复程序或自动清理。
