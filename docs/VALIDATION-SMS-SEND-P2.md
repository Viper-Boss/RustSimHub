# SMS 发送验证（P2）

日期：2026-10-13。最终 Rust 源码 `bb910b5280d8a442e4f37924c99484069a2e6614`。

## 结果

| 环境 | 验证 | 结果 |
| --- | --- | --- |
| GitHub / Rust 1.98 | 格式、79 项测试、core 默认功能集、Clippy -D warnings | 全部通过 |
| NAS ARM64 / Rust 1.99 | workspace 默认检查、79 项测试、release、自检、CLI help | 容器退出 0 |
| 私有 peer D-Bus | 正常发送、再次 submit、Create 失败、Send 失败/超时 | Send 最多一次；不重建或重发 Unknown |
| 私有 peer D-Bus | 创建后 SIM 变动、客户端撤销 | 调用 Send 前拒绝；已知未发送草稿取消 |
| 私有 peer D-Bus | ACK 后 State=SENDING、随后 State=SENT | 先 Unknown，仅状态查询确认 Submitted，没有再次 Send |
| 私有 peer D-Bus | Unknown 后路径删除复用且内容相同 | 保持 Unknown，不误判旧操作成功 |
| SQLite | scope、幂等参数冲突、唯一许可、失效 gate、重新批准 generation、事务失败 | 全部通过 |
| SQLite | creating/sending 阶段重开 | Submitting 恢复 Unknown，不能取得新发送许可 |
| 棒子非 root 只读回归 | 现有 1 条短信、信号订阅、周期修复、撤销、SIGTERM、DB 重开 | 通过；对象/存储错误均 0 |
| 棒子独立临时写模式 | 持久幂等 prepare、sms_read 用户不得 prepare、单次运营商提交 | 通过 |
| 棒子真实发送 | 用户授权后，仅向 10010 发送一次 CXYE | 绑定对象核验 SENT，操作 Submitted |
| 棒子真实接收 | 本次新收到的 10010 回复 | 私有正文已保存；总计持久收件 2 条 |
| 私有备份只读核验 | 1 个 operation、1 个 Submitting 事件、1 个 Submitted 事件、attempt=resolved | 通过，无第二次提交记录 |
| 停止/旧服务 | 两个临时单元停止、socket 删除、私有文件 0600 | 旧 nassimhub-agent/MM/NM/rmtfs 均 active |
| 网络 | Wi-Fi/USB ethernet/GSM | Wi-Fi、ethernet connected；GSM disconnected，未启用蜂窝数据 |

79 项：auth 21、core 17、ctl 1、domain 7、policy 1、storage 27、wire 5。
一个 peer 发送测试逐一覆盖 8 种场景，测试条目数与场景数不同。
CI：[38330072162](https://github.com/Viper-Boss/RustSimHub/actions/runs/38330072162)。
NAS 最终 release 编译耗时 4 分 48 秒，限制 2 CPU / 2 GiB；这是构建耗时，不是系统性能指标。

## 实机范围

余额指令采用 [联通官方短信营业厅](https://iservice.10010.com/elecService/message.html) 的 CXYE，
只向 10010 提交一次。未给个人号码发送、未拨号、未解锁 SIM、未改变旧服务或分区。
实际回复为非空中文文本，包含话费/查询相关文字，无字面 Unicode 转义；不公开正文、金额或任何账户标识。
这里验收的是发送与接收链路，不把回复内容当作已核对的实时账单金额。

写模式使用 root 的独立临时服务和随机测试身份，保护系统为只读、只放开新私有测试目录，最长 240 秒。
非 root 只读回归另用 nassimhub 用户，最长 150 秒。生产非 root 发送授权/Polkit 与完整 systemd 安装尚未验收。
真实错误、超时、换卡、断电等破坏场景只在私有模拟或存储重开测试验证，不冒充实机故障验收。

RSS/HWM 样本：非 root 观察 6496 kB，发送/回复后 6568 kB（约 6.41 MiB），均 3 线程。
这不代表全系统内存，也不证明相较旧 Go 服务的性能提升。

## 产物与可追踪性

NAS 使用固定 ARM64 构建镜像，版本/镜像信息沿用前阶段。源包逐字节匹配 HEAD 的 46 个允许列表文件。

| 产物 | 字节 | SHA-256 |
| --- | --- | --- |
| source.tar.gz | — | f4c3d5aa41c926b01e3b08c7cc6c47d787d1fdfa318859f96190bbd444964671 |
| rustsimhub-core | 4937184 | 00e3d0ff8e11865db0487fb276ffcd155c7d5cd7a2d17c1b815aaef6c0e90379 |
| simhubctl | 791568 | 437f2836a8572ec978bbea18a1ada5af0fd772e3adb2a284a93aeefca7115b22 |

ELF aarch64 PIE；core 含 dev-fixtures，模拟 D-Bus 服务仅 cfg(test) 存在。默认功能集另经 CI/NAS 编译检查。
本机 `.local/artifacts/{rustsimhub-core-send-p2,simhubctl-send-p2}`，NAS 当前 `src/target/release/`，
棒子 `/opt/rustsimhub/dev-send-p2/`；只读回归安装到 `/opt/rustsimhub/dev-observer-send-p2/`。
这些均为开发产物，不是完整刷机镜像。

## 私有证据与接手

- `.private/nas-send-final-build.log`、`nas-send-artifact-final.log`：最终构建/测试/哈希。
- `.private/device-observer-send-p2.log`：非 root 只读回归。
- `.private/device-send-carrier.log`：真实一次提交、SENT、收到回复、关闭与旧服务检查。
- `.private/carrier-send-evidence.tar.gz`：已停止的新测试目录、DB/随机身份/会话/正文及一次标记；不得上传 GitHub。
- `.private/carrier-readback-summary.json`：只读数据库核验的无正文摘要。归档在下载后验证 SHA-256 一致。
- 棒子 `/var/tmp/rustsimhub-send-p2-evidence` 是私有目录定位文件。
- `/var/tmp/rustsimhub-balance-query-20261013.once` 已存在。本轮一次发送授权已经使用；禁止删除标记或重新运行查询脚本。

最早存储测试中的重新批准方式和中间 Rust 路径移动编译错误已修正；失败/中间构建记录保留在 NAS 自有 history。
最终测试及 CI 均为修正后的代码。余额回复私有核验脚本曾用错表名/编码，修正后只读核验成功，未再次触发任何硬件动作。

剩余：生产权限/系统单元、发送历史分页与事件外部 API、草稿清理/本地 tombstone、长短信和投递报告、
中文/emoji/存储满等实机验收，及后续通话/音频/网络/完整系统恢复与镜像集成。
