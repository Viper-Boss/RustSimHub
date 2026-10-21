# 修正版系统组合与离线镜像验收 P6V

2026-10-22。用户追加“再做一个”，本轮完成R01-B：修正镜像审计与当前灯控配置不一致的问题，组合包含音频守卫/恢复条件/灯控的系统，验证候选用户态与ext4镜像，并导出本地制品。没有连接NAS或棒子、刷写、初始化业务、拨号、发短信或使用蜂窝。

## 实际修复

首次完整组合被审计拒绝：`etc/rustsimhub/status-led.conf` 已使用Rust灯控的 `auto`，审计仍只接受早期 `NSH_STATUS_LED_ENABLED=yes/no`。

`audit-rootfs.py`现与Rust运行时一致，只接受精确字节 `auto`、`auto\n`、`off`、`off\n`，并要求普通单链接0644文件及16字节上限；链接、FIFO、空文件、旧键值格式、注释、额外空白、CRLF、NUL和非法字节均拒绝。检查在空文件/非普通文件的跳过分支之前执行，避免静默接受。Root所有权由组合器及镜像逐项所有者验证保证，没有给未知配置或任意脚本开口。

新增两项审计回归，完整rootfs工具 **36项通过**。原冻结2b的失败系统树保留，修复在新提交独立冻结，不就地修改失败制品。

## 来源与镜像

| 项目 | 精确记录 |
|---|---|
| 系统组成/构建工具/Root灯控来源 | `0c817c1af2d68380cfcdb7de54e0f14d67372baa` |
| 上述源码归档SHA256 | `366e73414246c5c28fe73330dfc4b7d4af8f28e0bfe1037c4a1f3d416787190e` |
| 业务组来源 | 保持已验证的 `2b3c49cc469bce5e0f59ae377f9a59db9f9d0a89`，归档5d61a141…，采用R01-A原开发签名身份的package7；Core/Edge/Native仍同源 |
| Root灯控 | 按组合源码0c817c1新构建，Rust1.98，9项测试及ARM/glibc≤2.41边界通过；SHA256 `92166a82a699120efa25f4bb16f88d48191370950f097f6edb5f8de74ce8cfe4` |
| 镜像大小 | 3436183552字节 |
| 镜像SHA256 | `05029f1424e5ad1d85ad0d991f322fda49832ef3e99554a06a1f6fc38af2d852` |
| 树SHA256 | `784247ca5a70b54ff22800409aaa102a04b107cec6a06b1f6f63a3d45eedf87b` |
| 条目数 | 12373；ext4额外的空lost+found独立核对，不计入原树 |

没有把文档HEAD冒充制品源码，也没有修改2b包的build.json伪造新revision。组合器本就分别记录composition/application/LED来源：Root灯控遵守与组成源码一致的检查，业务签名组保持自己的同源归属。0c817c1与2b的灯控实现和Cargo锁字节相同，新编译是显式更新组件来源信息，程序仍需独立验收。

## 完成的检查

- 原通用基底、七业务Rust制品、Native、签名manifest及两份源码归档前后保持；独立Git bundle/冻结checkout未修改主工作区信任设置，最终组成源码干净。
- 完整通用镜像审计通过：无设备私有状态、SIM/短信/联系人、SSH私钥或厂商私有固件。选定签名公钥可随候选提供，私钥不进入镜像。
- 六个业务/维护Rust程序与Native在候选根文件系统中执行build-info，Core执行无硬件自检，来源和production fixtures=false匹配；额外在候选中以UID986执行新Root灯控build-info，总 **8个ARM程序**。
- Native以UID984在候选内执行合成Opus和DTLS/SRTP：500帧编解码、双向各50帧、指纹校验、错误授权拒绝、撤销及媒体超时关闭通过；PCM未打开。运行在独立mount/network/PID命名空间，仅启用该命名空间loopback，没有外部网络或设备绑定。
- **18个systemd单元**静态依赖/执行文件检查通过，包含新灯控、应用组、USB、固件、音频和维护入口。主机分析Debian自带networkd/timesyncd单元时有两条`!!`弃用提示，日志保留；未改包文件，不宣称零警告或真实开机通过。
- 所有系统overlay与冻结Git字节/记录摘要一致；16项实际音频profile均为固定23位大写十六进制，守卫的23位检查已在镜像内；app-recover保留installation.complete条件。
- 灯控启用链接、二进制摘要/Root0555、配置Root0644、Core提示目录、Root更新目录与服务写路径匹配。这里只证明镜像接线，尚未证明真实未读/网页/升级灯光联动。
- 新ext4的只读e2fsck通过，以`ro,noload`独立挂载，逐项内容/权限/所有者/capabilities与12373项源树一致；源树保持。导出Windows工作区后再核对镜像大小和SHA256。

已有Native21项、workspace222项等旧源码证明继续由P6U/P7E引用，本轮不重复计为新运行。没有加载内核音频模块，也没有进行真实电话或浏览器声音验收。

## 保存位置与失败记录

- 总索引：`.private/r01b-system-composition-20261022-first.json`，passed=true。
- Windows制品目录：`.local/r01b-system-image-0c817c1-first/`，含rootfs.img、SYSTEM-IMAGE.json、SYSTEM-CANDIDATE.json、mounted-verification.json、SHA256SUMS及说明。
- 原WSL树/镜像：`/home/wzs11/.cache/rustsimhub/system-tree-r01b-0c817c1-first`、`system-image-r01b-0c817c1-first`。
- 组成/完整流程：`.private/r01b-compose-20261022-third.log`、exit=0、phase=completed。
- 运行/增强核对：`.private/r01b-system-tree-runtime-20261022-third.json`、`r01b-system-extra-20261022-first.json`。
- 镜像与导出：`.private/r01b-system-image-20261022-first-manifest.json`、`r01b-system-image-20261022-first-mounted.json`、`r01b-system-export-20261022-first.json`。
- 首轮本地Git clone被WSL所有权保护拒绝；改用精确Git bundle，没有全局放宽safe.directory。第二轮被旧灯控规则拒绝的完整树保留。
- 首轮Root灯控增量构建报告找不到源码中存在的staged_update；疑似共享缓存/归档时间戳问题。原日志/退出码保留，未改源码，改用独立新target及正常源码mtime后9项与ARM构建成功。
- 导出助手最初语法错误发生在任何导出操作前，修正后导出通过；没有覆盖既有制品或删除失败目录。

## 交付边界与下一项

这是 **内部开发rootfs候选**，不是完整设备备份、boot镜像或已获硬件验收的发行包。`flash_allowed/release_ready/board_boot_verified/recovery_verified`均保持false；没有把既有独立内核复现证据自动改写成此镜像的启动证明。首次安装需的设备固件、维护身份和持久状态依然是单独的私有输入。

棒子状态沿用2026-10-21最后一次实机记录，本轮未连接，因此不作新的在线健康判断；没有改动实际设备安装索引和已运行灯控的版本归属。业务交付/初始化、冷启动、真实短信与通话、灯光业务联动仍待做。累计10010保持38，蜂窝继续等换卡。

下一项从本次P6V索引选择准确镜像，重新核对当前设备状态与可读备份，再准备独立交付/初始化和实机验收。不得直接执行旧3da候选、重跑已消费安装计划，或把本文离线通过当作刷写门禁已解除。

本轮P6由7/12计至7.5/12，其余权重不变，整体 **75/100，约75%**。增加的是修正版完整镜像与离线验收；实机交付及启动的剩余点数保留。完成此单项后保存，不启动下一项。
