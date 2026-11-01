# 成员D：D24低权服务离线实现

2026-11-01。分支codex/gtw-ble-service-runtime-20261101，目标main。承接 #154 最终 `7008728412d92db5934bc862bab29a7615528fda`，正常合并main `97f9c52fa832591a5e260d3dc4181847a8b3dfe3` 得到 `7e92d35abf591a9ae098e26a1ea7d1d5e2a826fa`。全部正式源码/材料受测HEAD **`70ed4bed08e6675f7b01a4db94f9b309e4ef590e`**，tracked_dirty=false；其后仅本说明与产品文档。

[实现与边界](../ble/service-runtime.md)。新增显式service CLI、内核核验的不透明Core port、真实首回调绑定私有Device1 lease、连接丢失监测/关闭/迟到补偿、仅审查材料归档。没有默认开启或虚构A provider、Core正式监听、业务鉴权、Root安装。新可选依赖仅复用现有版本，根lock只增加BLE包到getrandom0.4.3/rustix/serde的3条边，不改变任何第三方版本/校验和。

新候选真正复现过注册时序失败：真实BlueZ在RegisterAdvertisement返回前送首个Write，原主机已取callback却未排空原注册job，Transport.connected返回Busy，实际callback AccessDenied；`work/ble-service-runtime-trace.log` SHA256 `a9ae9c49f5c186ed1ca8c7fd7f51f2754cc298674c8e4164a7a24505b204583a`。修正为只有原Transport完成注册后才消费callback，期限/权限/断言不放宽；同设备丢失测试及其余服务测试通过。不是旧生产漏洞。Release用例按原生命周期语义核对已有移除事实，预期只注销app，无重复广播注销。

真实命令：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy -p rustsimhub-ble --test core_port_uid --features core-port --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy -p rustsimhub-ble --test gatt_kernel_uid --features bluez-backend --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 python3 crates/ble/tests/run_core_port_uids.py
CARGO_INCREMENTAL=0 python3 firmware/system/ble/run_policy_kernel_uids.py --rust-gatt --rust-backend
python3 -B -m unittest discover -s firmware/system/ble -p 'test_*.py'
python3 -B firmware/system/ble/render_material.py --output work/ble-service-final/review.tar
CARGO_INCREMENTAL=0 cargo test -p rustsimhub-core --locked --offline
cargo fmt --all -- --check
```

整仓 **1433通过/0失败/6继承忽略**，BLE170项含本包新增12项（3绑定、2监测、7服务），不把合并main带来的其他成员新增测试计为本人新测试。严格整仓与两个manual target Clippy、默认Core（lib 15、binary 194）及fmt/diff通过。材料19项含本包新增3项。实际UID Core1001/BLE1002端口12场景全部通过，独立原GattBackendV1/固定D-Bus policy/真实FD的3角色试验全部通过；二者均精确70ed4be、tracked_dirty=false，无新增ignore。manual target不计入workspace1433。Python首次runner参数数量错误已修正，正式结果使用成功复跑而非初次失败。

| 正式证据 | SHA256 |
|---|---|
| workspace.log | 3a7016fcb02671b167f77bb112f0d5b93cadac145f717c0b76ba3435499eab00 |
| clippy.log | 8b01334fd027957b40069d1d63251c7449f18eeeb99b79701bb0ec8da7425075 |
| 端口UID container.log | 6f7d902a6420097e47e6d693cdfec1ea7bac55e51702aef0ec0040ebc5df5a36 |
| 原GATT/policy container.log | 84418c5d9e588a89acb13cbb58b1013a4c426cbc51427171fa86ef30f3bee06f |
| material.log | c7355db722f2975e0c71ee4f7aeecd63cfcd9cc26a0393757f326883580e7eec |
| 新审查归档 | aab031761d7b13fcf7f3ff6a2e0349088abe3ebc30d04e1d9649d2a330174213 |

容器均network=none/read-only/NoNewPrivileges，临时UID只在容器，无host NSS/账户变更；保留既有32pids/256M/2CPU最小4caps的端口夹具，原GATT夹具保持16pids/128M/2CPU及原3caps，非目标完整48M/16tasks/10%CPU预算证明。真实production NSS/cgroup、Root/C发行组装、A安全会话/Core启动、ARM/HCI/RF/无线/NM/实机配网、两端听音未运行。不读生产数据/凭据，不拨号/短信/普通蜂窝，不签名或部署，不改主线台账/其他任务。原AGPL/来源作者保留，Codex辅助，实耗未知。

D24**离线实现/材料提交**完成，完整步骤由21/26变为 **22/26已提交、剩4（D22/D23/D25/D26）**；不等于审核、主线集成或设备验收。D22/D23大量组件已有PR，但实际A装配和完整闭环未据此扣除；继续D25真实TLS/TURN/媒体，D26随新增实际路径补故障。固定P0–P7依PROGRESS-SCALE仍 **80%**；未修改分母/权重/全局表。远端CI按最终HEAD单独回读。
