# LED-02 前置核实：ModemManager 消失与重现时 Core 不退出、不触发 OnFailure

日期 2026-10-28。用户选择 LED-02 走“方案一”：先在隔离环境核实，再安排实机。本记录**只是隔离环境的结果**，没有操作设备，也没有执行 LED-02。

## 要回答的问题

在实机上用 SIGKILL 让 ModemManager 进入 `failed`，从而让灯控亮红，需要先排除两个风险：

1. Core 发现 MM 消失后**自己退出**。Core 退出码非零会被 systemd 记为 failed，触发 `OnFailure=rustsimhub-app-recover.service`，而这是每次开机仅一次的自动应用组恢复。
2. systemd 因为 core 对 MM 的 `Requires=`，**连带停止或重启 core**，再经由 edge/media 的依赖扩大影响。

## 结论

两项风险在隔离环境中都没有出现，前提是恢复时用 `reset-failed` 加 `start`。

| 核实项 | 结果 |
|---|---|
| 真实 Core（`--serve-device`）在 MM 被 SIGKILL、缺席 2～45 秒、快速抖动时是否退出 | **不退出**。每次探测都正常应答，MM 回来后线路都重新绑定 |
| 部署源码 `0edfc54`（应用 50）构建的 Core | 同上，全部通过 |
| SIGKILL MM 后 core/edge 是否被连带停止，OnFailure 是否触发 | **都没有**，core/edge 的 PID 不变 |
| `reset-failed` + `start` MM | core/edge 不受影响 |
| `restart` MM（对照） | **core/edge 被连带重启**，所以实机恢复禁止用 `restart` |
| `stop` MM（对照） | **core/edge 被连带停止**，所以实机禁止用 `stop` |

## 代码依据

`crates/core/src/authenticated.rs` 的 serve 主循环中，observer、sender、call controller、媒体授权、灯控发布器这五个任务中**任何一个返回，Core 都会带错误退出**（`break Err(...)`）。逐个核对：

- **observer**（`observer.rs` 的 `run`）：MM 断开后，`session` 返回，随后清空线路绑定，按 1→30 秒的退避重新连接。只有 u64 代次计数溢出时才返回，实际上永不返回。
- **sender**（`sender.rs`）：只要请求队列还开着就一直运行；队列的发送端由服务主循环一直持有。
- **call controller**（`calls.rs`）：只在收到停止信号、请求通道关闭，或线路绑定的发送端被丢弃时返回。后者由 observer 一直持有；MM 消失时，它只会收到绑定变为 `None`，然后刷新。
- **灯控发布器**（`status_led.rs`）：发布失败时忽略错误、继续循环，永不返回。
- **媒体授权**：和 MM 无关。

部署版本 `0edfc54` 到 main 之间，Core 只改了转发历史相关的请求路由、转发模块和 build-info 输出，上述主循环和任务代码完全相同。

## 隔离实测

环境：本机 WSL2 Ubuntu（内核 6.18.33.2，systemd 259），x86_64 debug 构建，rustc 1.98.0。

### Core 与假 ModemManager

夹具：[`tools/field/mm-loss/`](../tools/field/mm-loss/)。

- 私有 `dbus-daemon` 作为 Core 的系统总线（`DBUS_SYSTEM_BUS_ADDRESS`）。
- 假 MM（[`fake-mm`](../tools/field/mm-loss/fake-mm/src/main.rs)）：一个调制解调器、一张合成 SIM，没有短信和通话，`CreateCall` 一律拒绝；记录收到的每个方法调用。`Voice.ListCalls` 只会在 Core 绑定线路后被调用，所以用它判断“已绑定”。
- 真实 Core 以生产模式 `--serve-device` 运行，所有目录都在临时目录里；启动前与 `rustsimhub-core-prepare` 一样，先用 `rustsimhub-migrate init` 初始化历史归档。
- 存活探针：`--request-auth` 发 `identity` 请求。

场景（[`run_mm_loss.py`](../tools/field/mm-loss/run_mm_loss.py)）：

1. MM 不存在时启动 Core，保持 8 秒。
2. 4 轮：MM 出现 → 等到线路绑定 → SIGKILL → 缺席 15 / 2 / 45 / 10 秒（45 秒超过最大退避 30 秒）。
3. 5 次快速抖动：MM 启动 0.4 秒后即被 SIGKILL。
4. MM 再次出现、线路重新绑定后，对 Core 发 SIGTERM。

| 被测 Core | 源码 | 二进制 SHA-256 | 结果 |
|---|---|---|---|
| main | `f3a9d22` 集成树（Core 源码与当前 main 相同） | `17559536d6070b45c942cc4c7bbf62eff83cfa32980d4406c8d5a97b35bfc084` | 通过 |
| 应用 50 | `0edfc5488edbd791c35d48f4d68bd309873f855e`（`git archive` SHA-256 `ea63fc7ccc810d31f0cb8155cddadd88800096d40f41a9d071b6212e26908c6b`） | `f1afe9d7cd6d2da75a680523b943a2467e70657775f7d8201bd42a8e13a7bd89` | 通过 |

两次结果相同：
- Core 全程没有退出，7 次探测全部正常；
- `Voice.ListCalls` 5 次，对应 5 次重新绑定；`CreateCall` 0 次；
- SIGTERM 退出码 0；
- Core 日志为空，没有 panic 或错误输出。

**重新绑定的耗时**：MM 回来后，Core 在 3.5～30.5 秒内重新绑定，缺席越久，退避越长。实机还要再加上真实 MM 重新探测调制解调器的时间。

第一次运行失败过：夹具漏了归档初始化，Core 启动即以 `auth: Forbidden` 退出。补上与生产一致的 `migrate init` 后通过。这是夹具环境问题，不是 MM 相关行为。

### systemd 依赖语义

脚本：[`systemd-semantics.sh`](../tools/field/mm-loss/systemd-semantics.sh)。在 `/run/systemd/system` 下建四个临时替身单元（进程都是 `sleep`），照搬生产的依赖行：core `Requires=`/`After=` MM，edge `Requires=`/`After=` core，二者都配置 `OnFailure=` 一个只写标记文件的 recover 单元。以 Root 运行，结束后删除。

```text
1 start:                 mm=active/1510 core=active/1511 edge=active/1512 recover_runs=0
2 SIGKILL mm:            mm=failed/0 core=active/1511 edge=active/1512 recover_runs=0
3 reset-failed + start:  mm=active/1556 core=active/1511 edge=active/1512 recover_runs=0
4 restart mm (contrast): mm=active/1571 core=active/1572 edge=active/1573 recover_runs=0
5 stop mm (contrast):    mm=inactive/0 core=inactive/0 edge=inactive/0 recover_runs=0
```

清理后确认：残留单元 0 个，标记文件不存在。

## 隔离环境覆盖不到、需在实机开始前只读确认的

1. **设备上 ModemManager 的 `Restart=` 策略**：`systemctl show ModemManager -p Restart`。如果不是 `no`，SIGKILL 后 MM 会自动重启，单元处于 `activating` 而不是 `failed`；灯控会把 `activating` 当成“配置中”，亮的是**白色**而不是红色。这种情况下方案一不成立，要停下重新评估。
2. **设备的 systemd 版本**：`systemctl --version`。`Requires=` 的这一语义长期稳定，但仍记录下来。
3. **设备上 Core 的实际启动模式**：可能是带转发管理或首配的 `--serve-device-*` 变体。它们共用同一个 serve 主循环，结论适用，但要记录。
4. **真实调制解调器在 MM 重启后的重新探测时间**，以及期间电话和短信不可用。
5. 本次是 x86 debug 构建 + 合成 MM，没有在 ARM、真实 MM 或真实基带上运行。

实机步骤见[看灯场 LED-02](acceptance/session-led.md#led-02--红色故障)。**安排实机前需要你同意。**
