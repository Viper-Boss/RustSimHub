# A-28：forwarder硬件权限拒绝修复与失败矩阵

账号wnfhsj88，按用户指示接手成员A；用户本轮明确要求直接以PR提交修复。起始主线：80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d。修复/审计工具/说明完整HEAD：7a6da18acb56f4cb62550aeef6e3ca5b21068bc5；后续仅增加本交付记录，最终PR HEAD见GitHub。

实际修复`firmware/system/overlay/etc/polkit-1/rules.d/45-rustsimhub.rules`一处账户列表遗漏：forwarder对两个硬件管理命名空间由项目规则fallthrough改为明确NO。未改core原五项允许、其他账户/动作、既有有限systemd规则或业务授权；不安装/加载规则。

新增`tools/security-matrix/`的实际规则回调矩阵、有限基础unit声明审计及SHA清单；说明`docs/security/member-a-failure-matrix.md`。不是用副本状态机测试Auth/Core。只读14份公开候选源，清单明确base与唯一变更文件的原/新摘要。

对应实现HEAD实际验证（工作区激活Node24.19/Rust1.98/Python3.12）：

- `node --test tools/security-matrix/test-audit.mjs`：7测试通过。真实两份polkit规则112格修复前105匹配/7处fallthrough，修复后112全部匹配，其中forwarder七格全部NO；另测声明变更、未知/反向/重复表达式、SHA变化、VM期限及CLI默认不读源。
- `node tools/security-matrix/audit.mjs --run`：**预期exit2/incomplete**。12份unit、112格规则，0条预期声明差异、仍有2项地址族未声明（app-select/recover）。不是全部安全通过；没有擅自调整更新器单元，实际OS/drop-in/UID/cgroup/Root/polkit加载待主线。
- `cargo test --offline --locked -p rustsimhub-auth --lib`：44通过，0失败/忽略。
- `cargo test --offline --locked -p rustsimhub-core --bin rustsimhub-core forwarding::ipc::tests::`：12通过，0失败/忽略。
- `cargo test --offline --locked -p rustsimhub-core --bin rustsimhub-core recovery_ipc_contract_tests::`：5通过，0失败/忽略。
- `cargo test --offline --locked -p rustsimhub-core --bin rustsimhub-core authenticated::tests::ipc_requires_proof_and_checks_revocation_even_from_same_uid -- --exact`：1通过，0失败/忽略。
- `git diff --check`通过。没有Rust实现/依赖变更，不重复声称其他PR整仓测试；此前误选Core --lib产生的0测试不计通过，以上为随后正确binary目标真实非零结果。

证据在工作区setup-logs的`member-a-security-{audit-tests,auth,core-forwarding,core-peer,core-auth}.log`及修复前/后静态JSON；不提交真实设备/身份/短信/凭据数据。

未运行真实Root、实际systemd/polkit全栈、设备/ARM/安装/蜂窝；未验证新增密码协议或可选schema7完整feature矩阵。规则矩阵使用合成subject而非实际OS授权证明，错误配置UID测试没有更改实际进程UID；这些边界不标已验收。下一步由负责人精确HEAD审核并协调实际安装策略的有限验证；本包仅PR交付，不自行合并/部署或更新全局完成率。

AGPL-3.0-only，无新依赖/第三方代码复制。OpenAI Codex辅助修复、审计与测试，保留原Auth/Core测试作者及历史。
