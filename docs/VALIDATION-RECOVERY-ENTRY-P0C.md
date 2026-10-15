# 恢复入口只读演练

2026-10-15，当前 UFI003 / Debian 13 / 6.12.49-msm8916-g93a71ee9468d，NAS 为 USB 主机。

先对此前的全 eMMC、boot0、boot1 私有备份重新独立完整回读，三份长度与 SHA256 均匹配原始捕获报告。正常系统中四个基础服务 active、全部 PCM closed，已存在呼叫均 terminated。

只执行一次 `reboot bootloader`。NAS 预先启动有限等待的探针；要求唯一 fastboot 设备的序列号与预先确认的同一物理 USB 端口一致，随后对明确设备查询并自动重启。探针只接受私有结果目录和 USB 端口，不接受刷机命令、分区内容或镜像输入。实现见 `tools/recovery-fastboot-probe.sh`。

实测产品 `LK1ST_MSM8916`；最大下载量 `0x10000000`；以下值与正常系统的只读分区布局一致：

| 分区 | fastboot 返回字节数 | 正常系统字节数 |
|---|---:|---:|
| boot | 0x4000000 | 67,108,864 |
| rootfs | 0xccd00000 | 3,436,183,552 |
| NSHSTATE | 0x80fbe00 | 135,249,408 |

`fastboot reboot` 收到 OKAY。约 53 秒后完成正常系统复核：boot ID 已改变，boot 分区 SHA256 与旧设备身份摘要保持，四个基础服务 active、全部 PCM closed、无活动呼叫，USB/Wi-Fi connected、普通蜂窝数据 disconnected。未新拨号、发短信或测试蜂窝流量，未安装生产自启单元，未写 boot/rootfs/NV/校准/GPT。无需用户拔插。

首轮在只读预检发现设备没有 Python，未启动探针、未执行重启；改为现有 shell 工具后完成第二轮。重启后 SSH 暂时拒绝/握手超时按有限等待处理，没有再次发出重启。

私有证据：`.private/recovery-entry-20261015-first.json`、`recovery-entry-20261015-second.json`（passed=true）、`recovery-fastboot-20261015-first.log`、`recovery-backup-readback-20261015.log`。包含设备身份摘要、主机端口或原始 USB 信息的材料禁止提交。

这验证了进入 fastboot、只读查询和返回当前系统；没有验证 EDL、故障系统的离线读取、完整镜像回刷或断电恢复。运行中全盘捕获仍不能冒充离线一致备份。`fastboot_entry_verified=true` 单独记录，`recovery_verified=false`、`release_ready=false` 和系统 A/B 未验证门禁保持。P0 增加 1 点至 6/8，总计 67.5/100，约 68%。
