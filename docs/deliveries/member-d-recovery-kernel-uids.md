# D22/26 局部交付：真实不同 UID 的 Core 门禁与 owned-channel

2026-10-30。Viper-Boss/RustSimHub，分支 `codex/gtw-recovery-kernel-uids-20261030`，
到main的依赖#101/#122草稿。起始#122完整HEAD
`663e138fcfce99a147d97ec2a59337d93f55f777`，正常merge#101
`51043fefcd323d6fd81f592da945f199249f5034`，无冲突，组合HEAD
`263f22c4658860c3a6136c29d9851921ac989027`；含main
`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。

手动来源/桥接验证与最终Core Clippy受测完整HEAD：
**`c4357d78b4b9c1dfb954e54a56804fd2b886487a`**，tracked_dirty=false。
整仓/默认包受测HEAD：`0ef46328d55fe6b5d58013fc9ba5289a1e19ba85`。
之后c4357d7只补显式手动target的写回/Close断言及启动器，不改runtime或普通整仓选中目标；
该变化在容器和Core全目标严格Clippy重验。最终完整HEAD见PR正文，后续仅本人文档。

## 突破的验证缺口

宿主无sudo/newuidmap，但已验证本地隔离Docker可用；用真实setuid进程，不伪造内核peer
或把单UID映射当多个角色。Core实际UID1001/GID1001、capabilities=0、no-new-privileges；
peer实际UID1002及其他UID，通过实际SO_PEERCRED、私有SourceGuard/RecoveryChannel进入Auth。
同测试socket组的Root0/Core1001/network1003/edge1004/forwarder1005/未知1006均拒绝；
合法来源但缺验证器仍拒绝。cfg(test)合成验证器注入后，实际peer nonce绑定能力，
真实跨UID读/写/prepare/worker/finish、单槽、Drop/Close失效、重连新nonce全部断言通过。
[具体矩阵、隔离和命令](../ble/recovery-kernel-uids.md)。

没有配置生产UID/GID/NSS/服务，只在临时容器创建fixture目录/进程身份。
Root设置器4项capabilities仅用于容器内setuid/gid/chown/chmod，Core自身实际为零。
断网、只读rootfs/输入、独立tmpfs、有限资源、唯一容器名和自身清理。
setup临时加入fixture组保留SGID后立即清空，没有增CAP_FSETID或账户组成员关系。
现有Core/Auth/Storage执行代码不变；只加明确手动cfg(test)target、两脚本和本人文档，
Cargo.lock/依赖/wire/schema/默认服务/Root材料无变化。

## 数量与结果

**手动容器1项通过、0失败，13组场景断言**，不是13项测试。
普通整仓 **950通过、0失败、6项继承忽略**（#122的945+#101新增5），
默认Auth/Storage87通过，默认Core库编译通过，整仓严格Clippy/fmt/diff通过。
手动target设test=false，普通suite不启动Docker/切UID，无新增ignore；显式容器结果单列，
不把未自动执行的target计入950。c4357d7的Core全目标严格Clippy通过。
所有Clippy保留既有proc-macro-error2提示，无新增忽略/迁就门禁的期限扩大。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 crates/core/tests/run_recovery_kernel_uids.py
# 未缓存镜像时显式加 --pull，使用固定公开digest，容器实际运行仍network=none。
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo clippy -p rustsimhub-core --all-targets --all-features --locked --offline -- -D warnings
cargo test -p rustsimhub-auth -p rustsimhub-storage --no-default-features --locked --offline
cargo check -p rustsimhub-core --lib --no-default-features --locked --offline
cargo fmt --all -- --check
git diff --check
```

Linux x86_64、Rust1.98、Docker28.4.0，本地daemon显式检查；构建锁定离线。
镜像固定 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`。
仅公开镜像获取使用平台现有代理/信任/认证，不更换endpoint或关TLS校验，不dump环境。
日志只在work/recovery-kernel-uids，最终手动二进制/runner/log摘要：

```text
binary 17e342431afa5ab75de4f14ca79c11efb44f7d55466651c8bae33f1a68823fe9
container runner 9a9a432b5c67ea627e824d37672d8cc921da7359d71e652cfcdc41089caeb661
container.log 3d799e90ceb7388ca9c7809bf807c1559a63485737e9b08e8fdf3d290ff5e64f
result.json 197af6c122f073359f333b0db66fc8255e4cfba3a58d4b47a5dbc14989c98df4
core-final-clippy.log 1835ecc274c9ab91fb5066a2b68e7112d09e8d22c10e962ca04708f0f73872ef
workspace.log c29c80a81c8ddb91240ad82c3a5bf0221f6e51a2462b27274a8dbe06bafd5898
clippy.log 613a0cb5a6339a625773d2e7b1774852a94a093b4866e391f3773315bd704cd3
default-auth-storage.log 6f35f37b42b6fe998dc826af15235eb33110befaba0022a5593c9d42eb6cce13
default-core.log 6b15e6ed8baad225bdb4da4ed2cd2d91049c03af739df3f1df0490bf2faaa75a
```

## 保留边界

真实的是容器内核来源/权限、真实Core/Auth/Store/owned worker/能力生命周期；
外部请求验证器仍cfg(test)合成，标准Ed25519测试Owner/grant不是A握手/AEAD互操作。
没有真实BLE/BlueZ/HCI/RF/生产NSS与角色UID、ARM设备/资源、Core-Wi-Fi业务派发，
没有生产设备/数据/NAS、短信/拨号/流量、签名/部署，缺A端口默认继续拒绝完整服务。
Docker测试不选择生产账户，不提升宿主用户或修改系统总线；仅自身容器/子进程清理。
release panic=abort边界未变，不宣称panic线程/进程隔离或本地Drop证明远端回滚。

完整步骤新增0，仍 **21/26离线包已提交、剩5步，固定P0–P7约80%**。
本批补掉“无法运行真实不同UID正例”的具体缺口，不能据此标D22完整验收。
AGPL-3.0-only新测试/Codex辅助，保留作者/许可，无工时虚报/全局台账修改。
前置未合并前草稿，不自动合并，CI按最终HEAD回读。
