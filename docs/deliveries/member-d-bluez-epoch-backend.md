# 成员D：真实BlueZ单epoch BackendV1交付

2026-10-31（Asia/Shanghai）。仓库Viper-Boss/RustSimHub，分支codex/gtw-bluez-epoch-backend-20261031，目标main草稿，依赖#130及其已有栈。起点#130 `7bfdfb121799ca79b82f62f9d222cb8e6b809869`；代码/全部正式受测完整HEAD **`a2eb75ada9dc243af46cd2fb20d9d210d5f8b623`**，tracked_dirty=false；后续仅本人说明/报告。

显式bluez-backend feature新增真正GattBackendV1实现，将既有固定manager/广播/GATT/PeerLease/FD组件接入原Lifecycle/Transport。提供专用连接单epoch的BluezBackend/BluezHost，once-only回调/Release/owner-loss接点；有界副作用槽、普通Drop立即本地失效，失败/取消/漏清理保留Unknown，真实迟到future继续补偿，不向替代owner清理。缺安全端口仍在导出/RPC前拒绝。没有默认启动、生产服务、A密码或Core-Wi-Fi业务实现。

[API、生命周期及限制](../ble/bluez-epoch-backend.md)。17项新增真实私有D-Bus测试实际运行；普通整仓 **967通过/0失败/6继承忽略**，严格整仓Clippy/fmt/diff、默认BLE编译通过。没有新增ignore。实际backend跨UID标准GATT手动1试验/3libtest角色各1通过；原直接模式3角色通过；旧头策略9场景与材料16项通过。手动不计入967；普通suite确认未运行两个手动target，未将角色数/场景数冒称普通新增测试。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo check -p rustsimhub-ble --no-default-features --locked --offline
python3 -B firmware/system/ble/run_policy_kernel_uids.py --rust-gatt --rust-backend
python3 -B firmware/system/ble/run_policy_kernel_uids.py --rust-gatt
python3 -B firmware/system/ble/run_policy_kernel_uids.py
python3 -B firmware/system/ble/test_inspect.py -v
cargo fmt --all -- --check
git diff --check
```

受测日志/结果摘要，逐输入二进制/镜像/宿主公共库hash由手动结果记录；仅本地work留存，原始二进制不入Git：

| 证据 | SHA256 |
|---|---|
| work/epoch-backend/workspace.log | b983ef9dfec2d8d806c2455a4dc16a2a0e924349f2a5883f043fb31e93cd99a6 |
| work/epoch-backend/clippy.log | 669399ed816de5a1352b441cf432636af253e85968452371e1fa006972b3a8b9 |
| work/epoch-backend/default.log | 8fd2f238e6e311a084c56e2a641c8b18ce631a72ebadb9ed425a35375e2cbb4e |
| work/epoch-backend/material.log | 2963f07d87b1abf6a0a92be00fe57b31192d019855ba5362598dbac7cb9b15eb |
| work/bluez-backend-integration/container.log | 71601342d6c6b15b58cdc6e3f8a2cd5370a0fb7173e24977f9055bfeb03779a4 |
| work/bluez-backend-integration/result.json | d13365df4055797e8a13a239f9406661289c948c05cd488a8e676f72b60af5ba |
| work/gatt-policy-integration/result.json | 5bdf54ef4b757a28f96559ea84d1692a9645edb30886f0668076918aa4a8871e |
| work/ble-policy-kernel-uids/result.json | 875eeb2ccda76ccfc8ce5cf5abc346243239cb0da8e2e2a4c7bf27f1504c1438 |

外部BlueZ Manager/Device1及安全/host测试端点合成，真实Rust backend/协议/FD/不同UID并不能证明实际A握手、Core恢复权、Wi-Fi/媒体、浏览器、HCI/RF、生产NSS/Root/ARM或48M服务资源预算。三项setupcap只在断网/只读/有限容器，四个真实角色0cap/no-new-privileges，候选policy原字节未改；不选/预留UID982、不改宿主NSS/system bus/Root角色或设备。release panic=abort，普通Drop/取消测试不是panic隔离或远端回滚证明。

默认feature空，Cargo.lock与依赖版本不变；无wire/schema/发布格式或生产默认值改变，无全局台账/他人分支/贡献归属更改，无真实业务电话/短信/蜂窝、生产数据库/NAS/秘密、签名或部署。原AGPL-3.0-only/来源作者保留，Codex辅助，无工时虚报。远端CI按最终HEAD回读，启动前账单失败不写为通过。

完整步骤新增0；仍 **21/26离线包已提交、剩D22–26五步，固定P0–P7约80%**。真实BackendV1缺口本包已补；A安全会话、单一Wi-Fi actor/迁移审批、生产Root角色/完整host和实机、TURN准入/实际适配与听音仍分别保留。下一包推进固定TURN源码准入审查，先补API/安全/资源证据，不擅自接未批准依赖。
