# 看灯场 · LED-02～06

**状态：材料，未执行。** 编写时没有操作设备。共用门禁、只读快照工具和证据模板见[现场包目录](README.md)。LED-01（联网原色）要用流量卡，放在换卡场。

依据：[指示灯约定](../status-led.md)、[P7E 实机安装记录](../VALIDATION-STATUS-LED-P7E.md)，以及灯控源码 `crates/status-led/src/{lib,linux}.rs`。P7E 只验收过安装和白色慢脉冲；网页、真实未读和升级的联动都还没有实机验收。

## 覆盖与不覆盖

| 动作 | 本现场 | 说明 |
|---|---|---|
| LED-03 青色读写 | 做 | — |
| LED-04 白色配置 | 做网页配置部分 | USB 初始化、首次配置的白色只在开机时出现，随下一次计划内重启顺带观察，本次不计 |
| LED-05 未读跑马灯 | 做 | 未接来电可以和听音场 01-b 共用同一通电话 |
| LED-02 红色故障 | **需要你单独同意后再做** | 用 ModemManager 制造故障；隔离核实已通过，见下文 |
| LED-06 紫色升级 | **不单独做** | 只能跟着下一次真实的签名应用组升级观察；本次只核对“没有候选时不出现紫色” |

## 颜色和时序（来自源码）

三个通道 `/sys/class/leds/{red:,green:wan,blue:}`，亮度只有 0/1。`led-trace` 按“红绿蓝”顺序输出 `rgb=`：

| 颜色 | rgb | 颜色 | rgb |
|---|---|---|---|
| 红 | 100 | 青 | 011 |
| 绿 | 010 | 紫 | 101 |
| 蓝 | 001 | 白 | 111 |
| 黄 | 110 | 灭 | 000 |

- 慢脉冲（白色配置、紫色升级）：6 秒一轮，亮 2 秒、灭 4 秒。这是开关式脉冲，不是渐变。
- 青色读写：亮约 120 毫秒，两次之间至少隔 5 秒。
- 未读：每种颜色 280 毫秒、间隔 20 毫秒。短信是“蓝→青→绿”，未接来电是“红→黄→紫”，两者都有时连跑六色共 1.8 秒。从状态变化那一刻起，每 30 秒一组。
- 优先级：关闭指示灯 > 故障红 > 升级紫 > 配置白 > 未读 > 新版本紫闪 > 读写青 > 联网原色。
- 联网原色（本现场只读记录，不验收）：仅 Wi-Fi 且 `wlan0` 连通为蓝；没有可用互联网为黄。

**两种观测方式都要用。** 你本人看实物，确认混色（黄、青、紫、白）确实能看出来，这一点只能靠人眼；`led-trace` 记录精确时序（约 20 毫秒采样，单位厘秒）。两者不一致时，以人眼为准记下差异，不改写成通过。

## 需要准备

- **你本人**在棒子旁，能看清指示灯。
- **浏览器 A**：所有者浏览器，具备 `network.manage` 权限（打开配置面板会发出配置提示）。
- **另一个未登录的浏览器窗口**（无痕窗口即可），用于“未授权页面不能改灯”。
- **第二部手机**：由你本人给棒子上的 SIM **发一条短信**、**打一通不接的电话**。这些都是来电和来信，设备不对外发送任何东西。
- **负责人维护通道**：运行只读快照和 `led-trace`。LED-03 需要一次有界的临时文件写入，现场先说明并取得同意。
- 预计 45～60 分钟。

## 开始前

1. 运行 `rsh-readonly-snapshot.sh once`，确认：
   - `led trigger=none,none,none mode=auto installed=yes`，`rustsimhub-status-led` 为 `active`；
   - `led-update absent`，或者 `available=false upgrading=false`；
   - `led-input` 里的 `unread_sms`、`missed_calls` 为 `false`。如果已经是 `true`，先在网页里查看那些记录、等它变成 `false`，或者把它记为起点状态。
   - 记下 `led rgb=` 的原色（预计 `001` 蓝，或 `110` 黄），以及 `net` 一行的 Wi-Fi 状态。
2. 运行 `led-trace 60`，在没有任何操作的情况下记录 1 分钟作为基线：记下青色出现了几次，以及有没有紫色。没有候选版本时，整段都**不应出现** `101`。

## LED-03 · 青色读写与低频闪烁

1. 维护通道开始 `led-trace 30`。
2. 5 秒后，在维护通道执行一次有界写入（约 1 MB）并删除：
   ```sh
   dd if=/dev/zero of=/var/tmp/rsh-led-io-test bs=64k count=16 conv=fsync && rm -f /var/tmp/rsh-led-io-test
   ```
3. 你本人看灯：应看到一次短暂的青色，随后回到原色。
4. 只读检查灯控自身有没有在刷日志：
   ```sh
   journalctl -u rustsimhub-status-led --since "-10 min" --no-pager | wc -l
   ```

**判据**：写入后 5 秒内出现 `011`，每次持续约 12 厘秒（受采样影响，10～20 厘秒都可接受）；任意两次青色的起点间隔不少于 5 秒；之后回到原色。基线和本次 trace 都没有出现“每 5 秒一次”的规律性青色循环。灯控在 10 分钟内没有周期性日志。

## LED-04 · 配置白色、退出与到期

浏览器 A 登录。每一步都开着 `led-trace`，并在步骤之间跑一次 `once`，看 `led-input configuring=`。

| 编号 | 操作 | 预期 |
|---|---|---|
| 04-a 打开配置 | 展开“网络状态、流量与配置” | 5 秒内开始白色慢脉冲（`111` 2 秒 / `000` 4 秒）；`configuring=true` |
| 04-b 结束配置 | 点“结束配置” | 几秒内回到原色；`configuring=false` |
| 04-c 切到后台 | 重新展开面板，再切到别的标签页 | 页面隐藏后回到原色；切回来后重新出现白色 |
| 04-d 关闭标签 | 面板展开时直接关闭标签页 | 回到原色 |
| 04-e 退出 | 重新登录、展开面板，再点“退出” | 回到原色 |
| 04-f 异常断开 | 重新登录、展开面板，然后断开电脑的 Wi-Fi（或强制结束浏览器进程） | 白色最多保持约 35 秒（服务端 25 秒到期，再加 Root 端 10 秒的新鲜度检查），然后回到原色。记下实际秒数 |
| 04-g 未授权页面 | 在未登录的窗口里打开设备网页，展开同一个配置面板 | 灯**不变**，`configuring` 保持 `false` |

**判据**：慢脉冲周期约 6 秒；04-b～04-f 都回到当时真实的下层状态（原色，或者正在播放的未读提示），没有残留白色；04-g 不改变灯。04-f 实测超过 60 秒仍未恢复时，记为未通过，不要重启服务来“修复”。

## LED-05 · 短信与未接来电跑马灯

| 编号 | 操作 | 预期 |
|---|---|---|
| 05-a 未读短信 | 第二部手机给棒子 SIM 发一条短信；之后开 `led-trace 70` | `unread_sms=true`；70 秒内出现两组“`001`→`011`→`010`”，每种颜色约 28 厘秒、间隔约 2 厘秒，两组起点相隔约 30 秒 |
| 05-b 查看后停止 | 网页“短信 · 日常使用”里刷新，对这条短信点“查看” | `unread_sms=false`；之后的 30 秒周期不再出现短信组 |
| 05-c 未接来电 | 第二部手机拨入，响铃约 10 秒后挂断，网页不接 | `missed_calls=true`；出现“`100`→`110`→`101`”组 |
| 05-d 两类同时 | 未接来电还没确认时，再发一条短信 | 出现六色连跑，约 1.8 秒 |
| 05-e 高优先级压制 | 两类未读都在时，展开配置面板 | 只看到白色慢脉冲，**不播**未读组；点“结束配置”后，下一个周期恢复未读组 |
| 05-f 确认未接 | 展开“手动调试与设备管理”，在其中的“电话”列表里打开那通未接来电的详情；然后点“结束配置” | 打开面板期间灯是白色（这个面板也算配置面板）；`missed_calls=false`；结束配置后，未读组完全停止（前提是短信已在 05-b 查看过） |

**判据**：颜色顺序和时长与上表一致；每 30 秒最多一组；查看/确认后对应的组停止；配置白色期间不播未读组。接通过的电话不应被当成未接，可用听音场 01-c 那一通交叉核对。

**已知缺口（源码核对，非实测）**：网页里只有两处会发送“已读/已确认”：短信是“短信 · 日常使用”里的“查看”；未接来电只能在“手动调试与设备管理 → 电话”列表里打开详情来确认。“电话 · 日常使用”没有确认未接来电的入口，普通用户不太可能找到这一步。这不影响本现场按上表执行，但要单独记录下来，作为日常电话页面的改进项。

## LED-02 · 红色故障

灯控在以下单元任一进入 `failed` 时亮红：`rustsimhub-firstboot`、`rustsimhub-usb`、`rustsimhub-device-firmware`、`rustsimhub-audio-drivers`、`rustsimhub-radio-resume`、`rmtfs`、`ModemManager`、`NetworkManager`，以及安装完成后的 `rustsimhub-core`、`rustsimhub-edge`。这些单元里只有 ModemManager 能比较可控地用来制造故障：

- **core、edge 不能用。** 它们配置了 `OnFailure=rustsimhub-app-recover.service`，会触发**每次开机仅一次**的自动应用组恢复，消耗恢复预算，还可能切换应用组。
- **开机硬件准备的 oneshot 不能用**（device-firmware、radio-resume、audio-drivers、firstboot、usb）。重跑它们会触碰固件、射频、音频驱动或 USB 维护链路。
- **NetworkManager、rmtfs 不能用。** 前者可能切断 Wi-Fi/USB 维护连接，后者和基带固件服务相关。

你选择了方案一：用 ModemManager 做，先在隔离环境核实。核实已完成，见 [LED-02 前置核实](../VALIDATION-LED02-MODEMMANAGER-LOSS.md)：真实 Core（包括部署源码 `0edfc54`）在 MM 被 SIGKILL、长时间缺席或快速抖动时都不退出；systemd 在 SIGKILL 以及随后的 `reset-failed` + `start` 下，既不连带停止 core/edge，也不触发 OnFailure；但 **`restart` 和 `stop` 都会连带重启或停止 core/edge**。

**实机执行需要你单独同意**，并且最好放在看灯场最前面，或者单独一个时段。代价是电话和短信中断约 1～2 分钟，期间的来电会漏接。

### 开始前（只读）

1. `systemctl show ModemManager -p Restart -p ActiveState -p NRestarts`：**`Restart` 必须是 `no`**。如果不是，SIGKILL 后 MM 会自动重启，单元处于 `activating`，灯会亮白色而不是红色。这时停止，不执行 LED-02。
2. `systemctl --version | head -1`；`systemctl show rustsimhub-core -p MainPID -p ActiveState -p NRestarts`，记下 core 的 MainPID；`systemctl show rustsimhub-app-recover -p ActiveState`，应为 `inactive`。
3. `rsh-readonly-snapshot.sh once`：`calls none`，PCM 全 `closed`，没有进行中的短信发送，灯为原色。
4. 你本人确认：接下来 2～3 分钟不打电话、不发短信。

### 操作

1. 维护通道开始 `rsh-readonly-snapshot.sh led-trace 120`（另开一个会话），你本人看灯。
2. `systemctl kill -s KILL ModemManager`
3. 保持至少 25 秒（灯控每 15 秒采样一次单元状态）。预期：`systemctl is-active ModemManager` 为 `failed`，灯变红（`100`）。
4. 恢复，**只能用下面这两条命令**：
   ```sh
   systemctl reset-failed ModemManager
   systemctl start ModemManager
   ```
   **禁止使用 `systemctl restart ModemManager` 或 `stop`**，它们会连带重启或停止 core/edge。
5. 等待 MM 重新探测到调制解调器（`mmcli -L` 能列出），以及 Core 重新绑定线路。隔离环境中，Core 在 MM 回来后 3.5～30.5 秒内完成重新绑定；实机还要加上调制解调器的探测时间。网页上点“检查状态”，确认线路恢复。

### 判据

- 灯在 MM `failed` 期间为红色，恢复后回到原色。
- core 的 MainPID 不变，`NRestarts` 不变；`rustsimhub-app-recover` 没有运行过（仍为 `inactive`，日志里没有本次运行记录）。
- 结束快照与开始时一致：`calls none`、PCM 全 `closed`、`gsm` 仍 `disconnected`；`rustsimhub-carrier-voice` 仍为 `active`。
- 可选的优先级核对：如果在 LED-05 查看短信之前做 LED-02，红色期间不应出现未读动画。

### 中途停止

- 第 1 步发现 `Restart` 不是 `no`：不执行。
- core 的 MainPID 变化，或者 app-recover 开始运行：立即停止所有操作，跑快照、记录，不再做任何写操作，交负责人处理。
- MM 在 `start` 后 3 分钟仍未探测到调制解调器：记录，不要反复重启 MM，交负责人处理。

## LED-06 · 紫色升级（跟随真实升级）

紫色只来自 Root 更新存储里**已验签**的 candidate，以及 stage/choose 期间的有期限活跃提示。不能伪造候选版本，也不能把 GitHub 新提交当成可升级固件。所以 LED-06 只能跟着下一次计划内的签名应用组升级做。升级本身的备份、验签、恢复预算等门禁，按升级流程执行，不在本包内。

在那次升级中加入这些观测：

1. stage 期间：`led-update upgrading=true`，紫色慢脉冲。
2. stage 完成、还没 choose：`available=true`，在原色上每 10 秒出现一次约 20 厘秒的紫色短闪。
3. choose 之后、confirm 之前：紫色慢脉冲。
4. confirm 或回退之后：紫色消失，回到原色。

**本现场只做负向核对**：基线和所有 trace 中都没有出现 `101`（除了 05-c 未接组里的紫色那一段）。

## 结束与还原

1. 确认没有打开的配置面板，`configuring=false`；`unread_sms`、`missed_calls` 都已回到 `false`。
2. 确认 `/var/tmp/rsh-led-io-test` 不存在。
3. 运行 `rsh-readonly-snapshot.sh once`，与开始时对比：服务状态相同，灯回到原色，`gsm` 仍 `disconnected`。
4. 测试短信和未接来电记录留在设备上，属于正常业务记录，不删除。
5. 按[证据模板](README.md#证据模板)写 `docs/VALIDATION-LED-FIELD-<日期>.md`，附各段 `led-trace` 的要点；原始 trace 放 `.private/`。只把判据逐条满足的动作记完成：本现场最多是 LED-03、LED-05、LED-02（如果你同意并执行了），以及 LED-04 的网页部分（USB/首次配置白色另记）。

## 中途停止条件

- 意外出现红色：立刻跑一次快照，看哪个单元 `failed`。这可能是真实故障，先查明，不重启服务。
- 结束配置后白色超过 60 秒不退：记录，不重启灯控或 edge。
- 灯完全不亮或 `trigger` 不是 `none`：停止，灯控可能没在运行，先只读查看 `rustsimhub-status-led` 的状态。
