# 修改提案：灯控与 network-agent 的轮询开销、meter.json 落盘

日期 2026-10-28。**只是提案，没有改代码。** 方案中凡是改变用户可见行为的地方都标为“需负责人决定”；归因还要在 STAB-03 结束后用只读测量确认（见第 5 节）。

## 1. 现象与证据

| 来源 | 灯控 `rustsimhub-status-led` | `rustsimhub-network-agent` |
|---|---|---|
| 开机以来平均 CPU（约 98 小时，按占一个核计） | 13.0% | 15.8% |
| STAB-03 开头 2 分钟实测 | 11.7% | 14.3% |
| 开机以来累计写盘（进程级） | — | 222 MB |

同期整机 eMMC 主机写入约 **1.4 GB/天**。按进程排：p15 的 ext4 日志线程 `jbd2/mmcblk0p15` 写了 862 MB（第一），journald 347 MB，sshd 229 MB，network-agent 222 MB，PID 1 210 MB。原始记录在 `.private/field-readonly-baseline-20261028-*.txt` 和 `.private/stab03-20261028/`。

注意：systemd 统计的服务 CPU 包含该服务 cgroup 里所有子进程。下面的归因是**根据源码和总量推断的**，还没有按进程拆分测量。

## 2. 源码中的成因

### 2.1 灯控（`crates/status-led/src/linux.rs`）

- **采样线程每 5 秒启动 3～4 个外部进程**（`refresh()`，第 106～188 行）：
  - 一次 `systemctl show`，查 8 个系统单元的 `ActiveState`；
  - 安装完成后再一次 `systemctl show`，查 core/edge；
  - 一次 `nmcli -t -f DEVICE,TYPE,STATE device`；
  - Wi-Fi 已连接时，再一次 `nmcli -g GENERAL.IP4-CONNECTIVITY,GENERAL.IP6-CONNECTIVITY device show wlan0`。

  `nmcli` 每次都要建立 D-Bus 连接、拉取 NetworkManager 的设备状态，在 A53 上开销不小。每 5 秒约 0.59 秒 CPU，折合每个子进程约 150 毫秒。
- 主循环每 20 毫秒一轮，每 200 毫秒读 3 个小文件，颜色变化时才写 sysfs。估计不到 0.5%，不是主因。20 毫秒的粒度是未读动画（280 毫秒 + 20 毫秒间隔）所必需的，建议保持。

### 2.2 network-agent（`crates/network-agent/src/linux.rs`，主循环第 622～712 行）

- **每 2 秒**：
  - `nmcli -t -f GENERAL,IP4,IP6 device show`，查所有设备的全部字段；
  - `nft -j list table inet rustsimhub_net`，读计数，输出完整 JSON；
  - 计量策略开启且已下发规则时，再加一次 `nft -j list quota`。
- **每 15 秒**：`collect_radio()` 调用 `mmcli -L`、`mmcli -m … -J`、`mmcli -i … -J`、`mmcli … --get-cell-info -J`。
- **每 5 秒无条件写 `meter.json`**（第 705～708 行）：`publish()` 先写临时文件、`sync_all`、rename，再对目录 `sync_all`，目标在 p15 的 `/var/lib/rustsimhub/rustsimhub-v1/system/network`。**即使没有任何流量、内容完全没变也照写不误。** 每次 fsync 都会强制 ext4 提交一次日志，这与 `jbd2/mmcblk0p15` 居写盘首位相符。目前普通数据是关闭的，计数器长期不变，这些写入几乎全是重复内容。
- 每 2 秒的 `/run/rustsimhub-network/status.json` 也走同一个 `publish()`。它在 tmpfs 上，fsync 代价很小，属次要问题。

## 3. 方案

### A. `meter.json` 落盘（收益最大，建议最先做）

| 方案 | 内容 | 断电时丢失多少计量 | 改动范围 |
|---|---|---|---|
| **A1 内容变化才写（推荐，第一步）** | 记下上次落盘的序列化字节，没变化就跳过；策略、SIM、月份切换仍立即写 | **不变**：有流量时仍最多丢 5 秒 | 小；只改 `linux.rs`，加单元测试 |
| A2 有流量时也降频 | 变化量达到阈值（例如 4 MiB），或距上次落盘满 60 秒才写；最短间隔仍是 5 秒 | 从“最多 5 秒”变为“最多约 4 MiB 加 5 秒流量，或 60 秒低速流量” | 小；需改 `accounting_note` 文案和 NET-05 掉电判据 |
| A3 `/run` 状态不 fsync | tmpfs 上的 `status.json` 跳过 fsync | 无影响（`/run` 重启即清空） | 很小 |

A1 不改变任何对外语义，普通数据关闭时，计量文件的写入会降到接近零。**A2 要不要做需负责人决定**：断电后已用量会少记，下次开机重新下发的内核配额会相应偏大，最多可能超出限额约 4 MiB。现在普通数据长期关闭，A1 已经足够，A2 可以等换卡做实际流量测试时再评估。

### B. network-agent 轮询

| 方案 | 内容 | 取舍 | 改动范围 |
|---|---|---|---|
| **B1 降频并缩小查询范围（推荐）** | 设备信息改为 10 秒一次，并只查需要的字段（`-f GENERAL.DEVICE,GENERAL.TYPE,GENERAL.IP-IFACE,GENERAL.STATE,GENERAL.CON-UUID,IP4,IP6`）；计数改为 5～10 秒；无线信息改为 60 秒 | 网页状态最多滞后 10 秒；速率显示变成 5～10 秒平均；限额触发后断开 NM 连接最多晚 10 秒。**内核 nft 配额仍会立即丢包，不会超量** | 中；只改 `linux.rs`，不加依赖 |
| B2 改用 D-Bus 事件 | NM/MM 状态改为订阅信号，计数仍由 nft 读取 | 节省最多，但要在 `network-agent/Cargo.toml` 加 zbus，测试面更大 | 大；需改依赖 |

### C. 灯控

| 方案 | 内容 | 取舍 | 改动范围 |
|---|---|---|---|
| **C1 降频（推荐，第一步）** | 两次 `systemctl show` 合并为一次，改为 15 秒；设备状态和连通性的 `nmcli` 也改为 15 秒 | 故障红灯和“activating”白灯最多延迟 15 秒才出现。LED-02 实机步骤里“保持约 15 秒”要改成至少 25 秒 | 小；只改 `linux.rs` |
| C2 复用 network-agent 输出 | network-agent 在 `/run` 状态里补一个 `wlan0` 连通性字段，灯控读该文件，不再自己调 `nmcli` | 灯控对 network-agent 产生依赖：状态文件过期或缺失时，按“没有互联网”显示黄灯，属于保守处理；要不要把 network-agent 失败也纳入红灯判断，需负责人决定 | 中；两边各改一点 |
| C3 订阅 systemd D-Bus | 单元状态变化时即时更新，不再轮询 `systemctl` | 红灯/白灯反而更及时；需加 zbus 依赖，并核对沙箱（当前 `RestrictAddressFamilies=AF_UNIX` 允许连系统总线） | 大；需改依赖 |

## 4. 负责人与顺序

依据：T6 分工和[成员 B 任务书](team/MEMBER-B-NETWORK-ACCOUNTING.md)。成员 B 可以对 network-agent 的 `lib.rs`/`linux.rs` 做最小修正、加单元测试，但**不能改 `Cargo.toml`/`Cargo.lock`**。灯控生产代码归主线；成员 D 的 LED-07 只增加测试，不改生产代码。

| 顺序 | 内容 | 负责人 | 依赖 |
|---|---|---|---|
| 0 | STAB-03 结束后只读测量，确认归因（第 5 节） | 负责人（可交 Claude） | 无 |
| 1 | A1 + A3：meter.json 内容变化才写；`/run` 不 fsync | **成员 B**（NET-01/02 范围内的最小修正） | 0 |
| 2 | C1：灯控降频 | **负责人**（可交 Claude 在独立分支实现，由你审核） | 0 |
| 3 | B1：network-agent 降频并缩小查询范围 | **成员 B** | 1 |
| 4 | C2：network-agent 补连通性字段，灯控复用 | B 加字段，负责人改灯控 | 2、3 |
| 5 | A2、B2、C3 | 负责人决定是否做；涉及依赖变更的由负责人改 | 换卡测试 / 需求确认 |

每一步都应做到：普通数据关闭和计量语义不受影响；对外接口不变；失败路径（命令失败、超时、文件过期）有测试覆盖。

## 5. 归因确认与修改后的验收

### STAB-03 结束后（只读，一条命令，约 2 分钟）

1. 对灯控和 network-agent 的主进程，间隔 120 秒各读两次 `/proc/<MainPID>/stat`：第 14、15 字段是进程自身的 CPU 时间，第 16、17 字段是已回收子进程的 CPU 时间。两者分别相减，就能定量拆出“程序本身”和“外部命令”各占多少。
2. 同一窗口里读 `/proc/<pid>/io` 的 `write_bytes`，对象是 network-agent、journald 和 `jbd2/mmcblk0p15`；同时读 `mmcblk0` 的写扇区数，计算 10 分钟内的写入速率。
3. 如果测出来子进程占比低于一半，说明本提案的推断不成立，需要重新分析，**不按本提案实施**。

### 修改后的判据

- 灯控 CPU 不超过一个核的 2%；network-agent 不超过 3%。这两个是建议值，需负责人确认。
- 普通数据关闭、空闲时，network-agent 的进程级写盘接近 0；整机 eMMC 写入速率明显低于每天 1.4 GB，以实测为准。
- 再跑一次 24 小时长测，与本次 STAB-03 对照。
- LED-02/04/05 的颜色和时序不变；如果实施了 C1，要同步更新 LED-02 实机步骤里的保持时间。

## 6. 不在本提案范围

- 不改灯色规则、优先级、动画时序，也不改计量的月结和 SIM 归属规则。
- 不修 sshd 的 229 MB 和 journald 的 347 MB 写盘。前者大概与历次维护登录有关，后者与日志量有关，可在长测数据出来后另行分析。
- cron 缺少 `/etc/default/locale` 导致的 108 条错误日志属于镜像配置问题，另行处理。
