# D-26局部交付：蓝牙清理失败的责任不能被误判为完成

日期：2026-10-29。代码：`99a7f163c23e7afa7cb344bdad4b48b3d66e596d`。范围：实际 `Lifecycle`/`Transport` 库；未接真实BlueZ、A密码会话、Core恢复授权或设备服务，不代表D-26整项完成。

## 实际缺陷与最小复现

1. 启动真实Lifecycle，注册应用/广播并接入一个合成peer。
2. 请求停止；广播撤销成功，随后安全后端的 `disconnected` 返回Unknown/Backend/Deadline。
3. 生命周期正确保留Unknown，并继续尝试本epoch的应用撤销。
4. 应用撤销成功后，本地job/app/adv/peer句柄均为空；原 `is_quiescent()` 仅检查这些字段，错误返回true，遗漏已经置位的 `cleanup_failed`。

这会让遵循“retain until is_quiescent”的宿主误认为外部安全会话已确认清理；本库目前尚未部署完整BLE服务，因此本次不宣称实际设备已经发生这类泄漏。真实私有连接与外部清理确认仍由后续集成承担。

**失败基线已经运行并保留**：只新增上述测试、不修改生产代码。隔离Linux/UID65534运行实际 `rustsimhub-ble` 测试，1项失败，断言为 `unconfirmed security cleanup must remain owned`；38项原测试被过滤，不能算同批通过。源码归档 `d7f70b119c56fedf91565bc79f409f11f91444d5571af700dbd20b90ea486d4e`，日志 `8bfb63f3ad24f3d2eb1808d797f2ed66da60753e4b72721af38feb816f75ef56`。

## 修正与兼容边界

`Lifecycle::is_quiescent()` 增加已有 `cleanup_failed` 的否定条件。Transport本来组合了Lifecycle查询，因此自动保留同一失败状态。没有增加重试、延长5秒预算、制造成功回执、放宽安全端口或改变协议/权限/依赖。

失败后：Unknown保持、旧连接失效、新启动被拒绝，重复stop/poll不重试外部副作用；最后一个future结束也不等于清理成功。原先超时后**迟到的实际成功**仍可走到Disabled且quiescent，正常已确认的关闭不被永久阻塞。错误清理需要宿主明确处理，不能简单丢弃对象并偷偷另建实例。

## 新增4项实际库测试，25组结果向量

| 测试组 | 向量 | 实际检查 |
|---|---:|---|
| 原最小反例 | 3 | 停止时安全关闭返回Unknown/Backend/Deadline，D-Bus对象都撤销后仍不quiescent |
| 四种关闭入口 | 12 | stop/正常disconnect/owner丢失/广告Release × 三种错误；旧连接失效、新启动拒绝、不会重复Close/Unapp |
| 超时后迟到结果 | 4 | 保留未完成future；三种迟到错误仍Unknown，迟到成功才Disabled/quiescent |
| 实际Transport组合 | 6 | 三种关闭错误 × 有/无未完成安全回复；先建立真实结果缓存，关闭后不能再读，迟到回复不产生新通知，I/O结束后仍保留失败责任 |

测试运行实际生产状态机/缓存/队列。只有外部D-Bus和安全端口是cfg(test)夹具，opaque回复不充当密码或Core权限证明。没有用第二套状态机自证，也没有通过真实网卡、HCI或公网发送。

```sh
cargo test -p rustsimhub-ble --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-ble --all-targets --locked --offline -j3 -- -D warnings
cargo test -p rustsimhub-status-led --all-targets --locked --offline -j3
cargo fmt --all -- --check
python3 -m unittest discover -s tools/progress -p 'test_*.py'
python3 tools/progress/build.py --check
```

最终修正截面归档 `ed14e1016706d2557cdee090919ac7ed5d6dcf09b87d16fe1f67f2f731bd39be`，日志 `96b3cf8752ec8be9d19cfccac78ffc6543ee9fda4795b67edcd4fdc98539f482`：**BLE42+灯控24=66项Rust、严格Clippy/fmt、Python进度24项通过，0跳过**。无capabilities的UID65534、网络/设备隔离、只读源码/缓存、临时输出。后续仅修改发布入口/交付文档和公开看板数据，生产代码保持此提交。

## 未完成和记账

A安全会话、D-21恢复授权/迁移、D-22可信IPC、D-23 Wi-Fi接线、D-24实际服务、D-25真实TURN，以及D-26围绕这些新增生产模块的完整矩阵仍待完成。PR52/53/56的前置方案尚未获得独立审核，未自行批准实施。D继续20/26离线交付、余6步；SEC-06记进行中，不标done，不增加81.5%或20/169完成数。

交付记录、任务书来源投影及新公开入口更新后，另以冻结归档 `731ee2a8c12b0380a9d19bc735f4070ae0b5b2f51dbd1aaad7e8217fc053c6c7` 运行Python进度24项、0跳过及生成一致性，全部通过，日志 `45bcd9c5527c9986640d033fd88c76646bbd0e17d48b7bc82c9c022ba3f32844`；Node检查也通过。该次不重复运行Rust，生产代码仍为上述99a7f16。归档后仅补本段验证结果。
