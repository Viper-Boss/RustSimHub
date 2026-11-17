# A4 重启回执交付

日期：2026-11-17。对应 A4.1/A4.5 的 USB 共享配网、原principal、多端回执与重启边界。本轮离线实现/验证已交，不等于整个A4或设备验收完成；整体固定进度仍 **81.5%**。

- 起始main：`de663743bd726e77f7c560139d0f91ea78e8fe57`。
- 本轮父提交：`774d4a05a1ff5191297f5dfa0ca4e0d0a0f89b6b`（本人PR274原交付，尚未合并、暂无新审核评论）。
- `implementation_head`：`8576d880dea4dd3c97deb74bf7b8c90bfb323d49`。
- 实现树：`11e703c48a73de902399a07ce8b7267a87b3d26e`。
- 固定源码归档SHA256：`6d05dfd5895f834fcb104589dca25b0b2cc25c9351cb50ba72df8cb99b4d2cbe`。
- 后续报告提交的当前完整HEAD写PR正文，不在报告中循环引用自身。更新本人 `codex/offline-a4-usb-network-20261117` / PR274，不自审、自合并或部署。

## 变化与文件

原审计只有principal ID/generation，重启后没有可安全查询的完整绑定。现在新增只读 `wifi_request`；新试连在NM动作前落盘schema2原trial与完整principal/设备身份/持久realm摘要。查询前后均检查当前授权。没有完整结果的意图保留Unknown，历史成功明确不证明当前网络；旧回执不能确认/取消旧actor。

修改 `crates/wire/src/auth.rs`、`crates/core/src/authenticated.rs`、`network_trial/provision.rs`，新增 `provision/receipt.rs` 和 `provision/tests/restart_receipt.rs`，扩展原 `provision/tests.rs` 与 `tests/a4_usb_nm.rs`。没有修改protected recovery/BLE/AirSIM/声音、B的CI/发行入口、协调器、设备、全局台账或任务书。无新依赖，沿用AGPL-3.0-only，自有代码、AI辅助实现与核验。

接口、兼容、拒绝与回退说明见[只读回执契约](../offline-a/a4-usb-restart-receipt-20261117.md)。普通begin的schema2审计是新写入格式；旧schema1不删除、不迁移、保持防重，但不提供未经证明的历史查询。新只读命令不接受密码、SSID或任意路径。没有新恢复/签名/Root操作API。

## 实际验证

Ubuntu-24.04 WSL2，固定Rust1.98.0、locked/offline，生产feature `product-providers-candidate,email-production-candidate`。原始命令、退出码和日志随本地包保存。

| 检查 | 结果与边界 |
|---|---|
| 生产Core lib/bin `cargo test --no-run --message-format=json` | 各退出0，落盘测试副本与源产物哈希匹配 |
| `core-library-test network_trial::provision::tests::restart_receipt:: --nocapture` | 3通过/1显式child忽略，退出0；实际父测试启动新测试进程，在15秒上限内回收 |
| 新进程重建 | 关闭父Auth/Store，新进程打开原持久数据库/身份；旧token拒绝，重新签名登录读取原Saved；原key重复begin仍拒绝。child本身实际运行1项，不另累计为独立产品数量 |
| 不完整/旧格式/权限边界 | absent和legacy为Unknown；仅started/confirmation为Unknown；终态只读不覆盖；异principal/realm/generation、非终态结果、损坏关联和符号链接拒绝 |
| 默认与生产普通provision筛选 | 各9通过/1child忽略，退出0；不运行recovery/热点专用套件 |
| 私有D-Bus NM普通/隐藏17场景 | 默认和生产组合各实际运行1项顶层测试的17个场景，均退出0；终态后清除ACTIVE，原发起者仍能读；第二Client/Owner不能接管；撤权不能读；旧confirm被拒 |
| `cargo test -p rustsimhub-wire --lib --locked --offline wifi_request_is_bounded_read_only_and_has_no_replay_credentials -- --nocapture` | 实际1项通过，退出0；空/超长key、短token、SSID/confirm/path字段均拒绝 |
| `cargo fmt --all -- --check` | 最终快照退出0 |
| Core/Wire `cargo clippy --all-targets --features rustsimhub-core/product-providers-candidate,rustsimhub-core/email-production-candidate --locked --offline -- -D warnings` | 最终快照退出0 |
| Core/Wire `cargo check --lib --bins --target aarch64-unknown-linux-gnu --features <同上> --locked --offline` | 退出0，仅交叉编译，不是ARM执行或实机 |

NM运行环境只有lo、UID1000、CapEff/CapBnd=0、NoNewPrivs=1；私有dbus-daemon正常停止。schema2用于原USB试连真实actor和实际NmProvision接线，外部NM/MM为有限模拟，不访问系统bus或主机网络。读取回执不产生额外NM动作。仍保留激活回包丢失/隐藏标志丢失的Unknown，不声称清理了无法证明的profile。

首轮完整构建/检查在树 `f461594e0bb11e10fdb105329b05ef4fefb94b26` 通过；随后补设备公钥域和有界子进程，已对上列最终树重新构建和运行，不把首轮结果当最终证据。旧A4/A5报告按各自固定HEAD保留；A5固定源774d4a不自动升级为本次新HEAD的验证。

## 交接、限制、下一步

负责人按implementation_head核源码、验证后审核PR274；B可消费新Wire/能力字段及契约，但本轮没有改其CI/打包文件。工具需保留原key，重启后重新登录，按 `wifi_request_api` 调用；不得用新begin重做查询。外部工具/`configurator/`仍归原维护方，本轮未改。

回退本实现提交不清理schema2或schema1标记；旧运行时不能查询新op，但仍拒绝同key覆盖。无数据库迁移或自动回滚NM动作；需现场恢复的情况由主线检查原网络，不凭未知回执自动重发。

未新跑全仓、热点、真浏览器、QEMU、生产Core服务/整机断电或实际USB/隐藏路由器。此次真实重启指测试进程与Auth重建，不能写成实机重启验收。网页产品字节未改，先前浏览器证据只属于原固定HEAD。云端CI在最终HEAD另读，不用WSL成功代替。源码/测试包不含真实身份、通信、设备备份或生产秘密。

下一步为主线合并后组合复验、外部电脑工具消费新查询接口，以及真实服务重启/断电后的USB端到端验收。热点仍暂缓，其他A包待完成项不由此销账。
