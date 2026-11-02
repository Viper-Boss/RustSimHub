# 更新账本只读入口：代码与冻结构建验证

2026-11-02，主线UPD-01补充。产品提交为`b37af31d685ef9a843c5800fc59d1249cdc7fe22`；本记录不代表已经合并、安装或通过Root运行验收。接口见[契约](PROPOSAL-UPDATE-READONLY-STATE-20261102.md)。

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

## 尚未运行与阻塞

已编写14场景的真实Root/chroot命令行夹具，包括合法只读、缺失/损坏/待恢复账本、公钥异常、真实独占锁、非Root拒绝、9种错误分派，以及合法旧status恢复对照。该夹具、既有maintenance与签名格式兼容脚本本轮均未执行。

尝试启动这一隔离Root检查时，自动审批服务返回额度不足，审批未完成，命令没有执行；这不是该操作被判定不安全。没有改用其他入口绕过审批，也没有Root测试进程或成功报告可供轮询。

本轮仅做过既有设备文件元数据只读核对，确认实际信任模式为0444；没有上传或运行新程序、替换正式更新器、变更账本或业务服务。正式设备仍不能据本地新build-info宣称支持该入口。

下一步为审批可用后执行固定冻结程序的Root隔离夹具，再做受限的设备只读探针，核对账本/正式程序/服务和业务数据保持。通过后才考虑正式交付。人工测试仍放最后；不改成员范围、PMIC、分区或基带。

原固件验收81.5%、本轮20/169及加权10.6%保持；主线代码约90%是未加权粗估，不因测试或构建数增加验收分。

私有证据：`.private/update-readonly-source-20261102-first.json`、`.private/update-readonly-frozen-20261102-first.json`及五份对应日志。原始设备信息、程序和未执行的运行驱动保留在私有目录，不随本记录发布。
