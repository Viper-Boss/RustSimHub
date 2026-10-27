# L2 · DNS双协议端口预留夹具修正

2026-10-27，主线持续目标恢复后的CI修复。固定提交6f964ceb53a5d1effba9bf79748b327b7375b353的[CI40425761573](https://github.com/Viper-Boss/RustSimHub/actions/runs/40425761573)失败，native-audio成功，test作业在forwarder库的UDP截断转TCP测试失败。原因已有直接日志：egress/tests.rs原166行TCP bind返回errno98 / AddrInUse，77项通过、1项失败、1项ignored。没有把它归因于成员PR或真实DNS服务。

## 根因与修正

旧夹具先`UdpSocket::bind(127.0.0.1:0)`，然后用分到的端口绑定TCP。UDP临时端口分配不预留TCP端口，所以同号TCP可能已被另一个并行测试/系统连接占用；日志证明绑定冲突，没有证据断言具体占用者。

仅修改`crates/forwarder/src/egress/tests.rs`：

- 先预留TCP临时监听，再尝试同号UDP，并持续持有两只socket；端口不被先释放再重新绑定。
- UDP仍可能与其他UDP socket冲突，故仅在任何测试流量开始之前对AddrInUse最多尝试16次；其他错误立即返回，预算耗尽失败。
- 原UDP截断转TCP和总截止关闭连接两例使用同一个夹具。原真实DNS解析、同一个resolver/网卡/源地址和连接关闭断言保留。
- 新用例制造真实UDP占用，验证最终TCP/UDP地址相同；另一用例在全部候选制造冲突，断言16次预算，并用注入PermissionDenied断言不会重试其他错误。

这不是生产DNS/HTTP重试：没有修改产品解析器、绑定出口、超时/租约、授权、队列或网络策略，没有串行化所有测试、跳过失败断言或增加SO_REUSEPORT来掩盖冲突。

## 验证与构建缓存纠正

最初复用旧target，以冻结归档的固定mtime解压到原源码挂载路径。Cargo未重新编译该测试文件，运行清单仍只有旧10项，在第97轮旧deadline夹具再现AddrInUse。此轮判定失败并保留，不能冒充新修正不稳定或通过。

随后使用全新空target、只读冻结源码与依赖、实际UID65534/空能力/NoNewPrivileges、私有网络/设备进行离线构建。执行前强制读取真实测试ELF的`--list`并确认两个新用例存在；新组实际为12项。生产源码归档保持可复现的固定mtime时，不能仅依赖源归档SHA环境变量让所有crate自动重编译，必须使用新target或另有明确的输入指纹证明。

冻结工作截面734文件，SHA-256 `6756c4becbf6debaf08991c7422d677dae8976b6507822db6bfe717f6fb97179`。原始失败CI日志、新旧压力日志、测试清单和本机结果放在`.private/dns-fixture-l2-20261027-first/`、`.private/dns-fixture-l2-20261027-second/`及私有CI日志，公开只列脱敏结论。

本机新12项DNS组已连续100轮通过；另Core157、forwarder80、Wire18共255项普通测试通过，专用ENOSPC仍是默认ignored的独立用例，不混入普通通过数。严格Clippy初次指出冗余闭包，改为直接函数引用，保留该次失败；最终显式清理forwarder包的开发产物后重新构建并再次检查新测试清单，12项100轮及255项普通回归、严格Clippy、fmt和Core/forwarder/Edge生产检查均通过。最终735文件截面SHA-256 `a723dff904600592b75634d5b013c6085680eae3daca06b08f4034b8c5c822b2`，证据`.private/dns-fixture-l2-20261027-third/`；其他未变依赖缓存复用，不称全部target重新构建。新固定提交完整CI尚待核对，不用本机通过替代。本修正不增加169项台账的完成动作数；原80%、14/169、加权7.3%保持。UPD-01仍进行，四位成员扩展任务边界、普通数据/转发OFF和PMIC/关机搁置保持；未操作设备或真实提供商。

## 固定提交CI补记

2026-10-27：完整HEAD `5c6e4e29bcebf616e4c47889f38f3e4bcbb2bea9` 的[push CI40443986959](https://github.com/Viper-Boss/RustSimHub/actions/runs/40443986959)和[PR CI40444040928](https://github.com/Viper-Boss/RustSimHub/actions/runs/40444040928)均已核对test/native-audio全部success。修正通过[PR#4](https://github.com/Viper-Boss/RustSimHub/pull/4)交付；截至该次核对仍open、mergeable=true、mergeable_state=blocked，未合并/部署。自动审批拒绝直接推main后改走正常PR，不通过其他方式绕过。两个运行的完整步骤保存在私有固定CI记录，原失败40425761573保留。
