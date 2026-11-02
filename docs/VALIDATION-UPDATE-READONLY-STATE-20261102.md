# 更新账本只读入口：代码与冻结构建验证

2026-11-02，主线UPD-01补充。产品提交为`b37af31d685ef9a843c5800fc59d1249cdc7fe22`；Root隔离检查及设备临时受限探针已通过，正式更新器尚未替换。接口见[契约](PROPOSAL-UPDATE-READONLY-STATE-20261102.md)。

## 实际改动

新增Root专用、固定路径的`rustsimhub-update inspect-state`。复用现有账本解析及锁，在只读打开的既有锁上取得非阻塞共享锁，核对已安装公钥与账本绑定、文件身份和变化；缺少文件、待恢复state.next、符号/硬链接、特殊文件、权限异常或独占锁占用时拒绝。不初始化、恢复、选择或确认应用，不读取业务库。支持设备实际使用的0444公钥以及0644公钥。

报告明确区分账本信任绑定与授权、应用完整性、实际选择、健康；后四项均为false。它不代替成员C的候选健康评估，也不是维护授权票据。

原通用命令分派在动作和参数数量校验前就会打开可写Store；现在先校验，避免错误命令意外恢复state.next。合法旧命令保留原语义，旧status仍不是纯查询。

## 已完成验证

冻结源码归档SHA-256：`926075557d1482d67c4e24ce4f4e39b78a16ce215893e1ac0d9a306a6720e211`。在WSL私有网络命名空间中降权为普通用户执行以下命令，均退出0：

| 检查 | 结果 |
|---|---|
| `cargo +1.98.0 fmt --all -- --check` | 通过 |
| `cargo +1.98.0 clippy --workspace --all-targets --all-features --locked -j 8 -- -D warnings` | 通过 |
| `cargo +1.98.0 test --workspace --all-features --locked -j 8` | 1374通过、0失败、6项原有专用测试忽略 |
| `cargo +1.98.0 build -p rustsimhub-update --no-default-features --locked --offline -j 8` | 原生程序构建通过 |
| `cargo +1.98.0 build --release -p rustsimhub-update --no-default-features --locked --target aarch64-unknown-linux-gnu -j 8` | ARM release构建通过 |

新增9项实际文件系统测试，覆盖不写入/不建versions、0444信任、待恢复状态保持、缺失或损坏输入、读写锁互斥、复制描述符释放、观察期间替换、链接/FIFO/权限/大小和状态关系。它们在普通用户下验证库实现，不等于Root命令行或设备验证。

原生程序SHA-256：`41dd1a74672ce10844c2617e06c470361db40d2f63f12256b6efcf0029f90de3`。

ARM程序为2,895,480字节，SHA-256：`81d6aaa4c0017a2923da0840fa06eb28abda5c7c568de5b81429b873d903825a`。

`tools/test-update-state-inspection.py`已通过Python AST语法检查。CI中新增了该脚本步骤，但本轮未运行云端CI，不称云端通过。

## Root运行与兼容检查

已在私有网络命名空间、一次性chroot内运行同一冻结原生程序，以下三个检查均退出0，使用合成密钥和临时账本，不访问设备或修改宿主安装：

- `tools/test-update-state-inspection.py --updater ...`：14场景通过，包括合法只读、缺失/损坏/待恢复账本、公钥异常、真实独占锁、非Root拒绝、9种错误分派，以及合法旧status仍恢复state.next并创建versions的对照。拒绝与只读场景的完整夹具树保持。
- `tools/test-maintenance-observation.py --updater ...`：18场景通过，覆盖实际数据UID、锁/SQLite旁文件/损坏/链接和非Root等既有有限维护路径；仍不声称有效systemd或写入准入已由此验证。
- `tools/test-signed-application-formats.py --updater ...`：Rust与Python签名一致、格式1/2、六文件选择/五文件回退及损坏/未签worker拒绝通过；合成载荷未执行，不代表真实outbox回退。

首次启动曾因自动审批服务额度不足而未执行。后续独立Git同步通过审批后，重新通过原审批路径获准启动上述检查并成功；没有换入口绕过审批。先前提交中的“未运行”保留为当时历史状态，以本节和实际报告为最新结果。

## 设备临时只读探针

先核对实际账本、公钥0444及服务基线，候选只上传到唯一维护目录、Root0700/程序0500，并回读SHA-256及build-info。临时systemd进程以Root运行，仅执行inspect-state；设置ProtectSystem=strict、ProtectHome、PrivateTmp/Devices/Network、NoNewPrivileges、空能力、AF_UNIX、32MiB、4任务与8秒上限，执行成功后回收。

实际输出与设备既有应用71账本完全一致，公钥绑定为true，而授权/载荷完整性/运行选择/健康四项仍为false。前后账本目录清单及inode/权限/大小/mtime/ctime、公钥元数据和账本/公钥内容摘要相同；正式旧更新器摘要未变，九单元状态、八个常驻PID、一次性USB单元的active/exited状态、原boot保持。普通蜂窝数据断开、转发PID0、应用恢复预算未使用。没有读取或回滚业务数据库，没有停止业务、切网、外发、重启、分区或PMIC操作。

首次设备预检错误地要求USB oneshot的MainPID大于1，在上传前退出；只读复核确认其为正常active/exited。第二份驱动仅对这个确切单元要求Type=oneshot、PID=0、SubState=exited，其余常驻单元仍要求running和非零PID。首次失败报告保留，未被覆盖成成功。

这是临时程序的真实只读运行证明；正式更新器尚未替换、未增加可远程调用的Root接口，也不冒称健康确认或正式支持已交付。下一步按正常PR门禁交付代码，再决定正式更新器安装；人工测试放最后，成员及PMIC范围保持。

原固件验收81.5%、本轮20/169及加权10.6%保持；主线代码约90%是未加权粗估，不因测试或构建数增加验收分。

私有证据：`.private/update-readonly-source-20261102-first.json`、`.private/update-readonly-frozen-20261102-first.json`及五份日志、`.private/update-readonly-root-cli-20261102-first.json`及三份日志、`.private/update-readonly-device-probe-20261102-first.json`（首次预检失败）和`second.json`（真实成功）。原始设备信息、程序和运行驱动只保留在私有目录。
