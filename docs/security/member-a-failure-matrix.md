# A-28：硬件权限规则修复与可重跑失败矩阵

基线`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`；本包按用户“直接以PR提交修复”的指示修改项目polkit规则。精确候选实现HEAD与验证见交付报告，不安装/加载规则。

## 已修复的覆盖缺口

`firmware/system/overlay/etc/polkit-1/rules.d/45-rustsimhub.rules`原project数组只有core/edge/media，遗漏新增forwarder低权账户。对forwarder的ModemManager/NetworkManager动作，函数直接return，**没有显式NO**。它是否最终获权取决于设备上的其他规则；不能把fallthrough当拒绝，也不能由此声称已获得硬件权限。

修复仅将`rustsimhub-forwarder`加入该数组：保留core的原5项YES，forwarder与edge/media一样对两个硬件管理命名空间显式NO；其他OS账户/动作的处理不扩大。现有46-forwarding规则仍只允许core启动两个固定Root核对单元，forwarder不能凭本修复启动/停止服务。未新增Owner、scope、Root动作或泛化systemd权限。

## 实际规则矩阵与静态单元盘点

```sh
node tools/security-matrix/audit.mjs          # 默认计划，不读源文件内容
node tools/security-matrix/audit.mjs --run    # 当前预期exit 2，仍有静态未核验项
node --test tools/security-matrix/test-audit.mjs
```

工具只读随包列明的14份公开源码（12个基础unit、2个项目polkit规则），每文件<=64KiB、最多32文件；用固定SHA256校验后才分析。`sources.json`记录base_commit、候选变更文件和原/新hash，不将修改后的规则冒充主线原文。源码变更、重复清单、读失败/不支持表达式保留incomplete，不复用旧结论。相对路径固定在firmware/system，不接受用户指定文件、设备目录或私有输入；不运行shell/systemctl/pkcheck/D-Bus，不读取credential值，不写源文件或修改系统。

112格实际JS规则执行：4个项目账户×7个硬件动作，4账户×5个单元×4种verb，加4个外部账户/动作负控制。每次在Node VM中以合成subject/action执行真实规则回调，单段执行100ms上限，保留第一项明确决定；NOT_HANDLED与NO严格分开。VM只用于这两份可信公开规则的测试，**不是通用不可信代码沙箱**。隔离项目规则不能证明设备完整polkit栈的最终授权，合成subject.user也不是内核UID证明。

实际修复前105/112匹配预期、7格forwarder硬件请求NOT_HANDLED；修复后112/112匹配，其中这7格全部为NO。其余105格不改变，包括core五项YES、有限systemd启动、大小写/后缀/其他单元拒绝以及其他OS账户/动作不由本规则处理。

12个unit盘点身份声明、capability/ambient集合、地址族、写路径、credential名称和固定执行文件，检查实际NNP/ProtectSystem/ProtectHome/UMask及预期声明。仅支持简单的已知systemd子集；重复标量、反向集合、续行等不猜解释，路径只记录声明，不展开specifier/继承/挂载。命令参数与Environment不输出。凭据只核对现有声明、输出名称，不读取文件内容。

| 单元 | 身份声明 | bounding能力 | 地址族声明 |
|---|---|---|---|
| core | rustsimhub-core | 空 | UNIX |
| edge | rustsimhub-edge | 空 | UNIX/INET/INET6 |
| media | rustsimhub-media | 空 | UNIX/INET |
| network-agent | root | NET_ADMIN | UNIX/NETLINK |
| mdns | DynamicUser | 空 | INET |
| app-select | 默认root | DAC_READ_SEARCH | 未声明；有PrivateNetwork=yes |
| app-recover | 默认root | 空 | 未声明 |
| setup-ap | root | NET_ADMIN | UNIX/INET/NETLINK |
| setup-edge | rustsimhub-edge | 空 | UNIX/INET |
| forwarder | rustsimhub-forwarder | 空 | UNIX/INET/INET6 |
| forwarder-reconcile/status | root | 空 | UNIX |

当前静态报告仍为**incomplete/exit2**：app-select/recover这2个基础unit未声明地址族，需主线结合实际子程序/PrivateNetwork/发行环境审视，不能直接判断成可利用漏洞或机械加AF_UNIX破坏更新。0条预期声明差异不是“整机安全通过”。未自动修改更新器单元。

未核验：真实UID数字与sysusers部署结果、systemd供应商unit/drop-in/默认值与实际有效配置、真实capabilities/cgroup/namespace、设备完整polkit规则和D-Bus策略、credential权限/加载、Root进程与固定二进制来源。没有这些证据不输出production_approved或runtime_verified=true。

## 真实Auth/Core失败回归

本包不复制Auth/Core状态机、不添加私有hook；在临时数据库/身份/Unix socket和现有合成外围端口上运行现有实际代码测试。选择真正带测试的Core binary目标，`--lib`在这些过滤下为0项，不能计入通过。下面记录的是随后正确binary目标的真实非零结果。

| 命令（均加--offline --locked） | 真实结果 | 证据边界 |
|---|---|---|
| cargo test -p rustsimhub-auth --lib | 44通过，0失败/忽略 | 真实签名/设备pin/用途与scope绑定、错realm/身份、挑战重放/单调期限、撤销/重启、私有文件链接/权限等；默认feature |
| cargo test -p rustsimhub-core --bin rustsimhub-core forwarding::ipc::tests:: | 12通过，0失败/忽略 | 真实Auth/queue/grant/claim、阶段和精确payload摘要、SIM/Owner撤销/过期、lost许可/finish、重启保留Unknown、peer UID、角色/帧预算 |
| cargo test -p rustsimhub-core --bin rustsimhub-core recovery_ipc_contract_tests:: | 5通过，0失败/忽略 | 真实子进程内核peer UID/GID/PID、连接不是PID标识、错误配置UID拒绝、匹配UID仍需Owner证明、帧预算；子进程没有切换UID |
| cargo test -p rustsimhub-core --bin rustsimhub-core authenticated::tests::ipc_requires_proof_and_checks_revocation_even_from_same_uid -- --exact | 1通过，0失败/忽略 | 同UID真实IPC仍需签名/会话/权限且撤销立即拒绝 |

没有RecoveryGrant/schema7可选feature的全矩阵、新密码协议/远程Owner桥、实际Root/polkit执行或设备/蜂窝测试。既有测试通过不代表这些范围已验收；源代码所属作者保持。本包只声明修复权限规则覆盖缺口及离线回归通过。

无新第三方依赖/源码复制，AGPL-3.0-only；OpenAI Codex辅助修复、审计与验证。主线审核合并后另行在实际安装策略与完整OS授权栈核验；本包没有自行合并、部署或改全局完成率。
