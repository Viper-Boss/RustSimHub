# 转发关闭与迟到响应：真实进程集成验证

2026-11-02，主线FWD-13继续推进。本轮新增可复用的实际运行检查与CI步骤，没有修改生产Rust状态机、账户、权限、发送策略或数据库格式，没有接管成员离线实现。

## 固定来源

- 外部Python运行夹具：`2839fb64b36eb2c62f54b179b9dda35a221e381f`，归档SHA-256 `59c7a8bf0043d16a41f7929e924c70d2aee4bcb21449cb89c57ae0382f70501b`。
- 真实Core/forwarder构建：`89a98c15000bfe2498fbf029c8ab865eba131290`，归档SHA-256 `5ed880e974634c682de8e02e95cd1f9e6e889c85efc908bf6c028ddd8cf1b7ef`。普通WSL用户离线构建、fmt通过，测试程序显式启用`dev-fixtures`，不能当作生产发行制品。
- 两个来源的`crates/`、Cargo清单/锁和转发systemd模板逐文件字节一致。差异只在外部Python核验；复用刚构建并核对摘要的程序，不将不同归档冒称为同一来源。
- 准备阶段修正了管道EOF的观察处理，以及把已退休的非秘密worker证明误当作活凭据的检查。生产关闭约束未放宽。

## 实际链路与结果

运行在WSL2真实PID1/systemd及cgroup v2下。每例重新创建只有lo的独立网络空间、临时Core身份/Owner、持久SQLite队列、低权限worker、唯一临时单元与仅允许两项临时job的polkit规则。复用现有TLS/DNS测试接收端，生产Core/worker/Root服务管理代码实际执行；没有另写队列、权限或关闭状态机代替被测代码。

| 关闭时点 | 本地HTTP请求数 | Owner关闭确认耗时 | 持久任务结果 |
|---|---:|---:|---|
| DNS请求等待回复 | 0 | 0.199秒 | 已取得任务Unknown、待执行任务Cancelled |
| TCP已建立、TLS握手等待 | 0 | 0.194秒 | 同上 |
| HTTP正文已收到、等待回复 | 1 | 0.186秒 | 同上，不重投 |
| 已关闭后才尝试返回HTTP 204 | 1 | 0.197秒 | 原Unknown保持，不改成Confirmed |

每例均核对：

1. 由实际Owner请求触发Core→D-Bus→polkit→固定PID1 job，不能只修改开关布尔值。
2. Root报告绑定当前代次且`process_state_verified=true`，原worker PID消失、MainPID为0、整个cgroup无进程。
3. 持久启用标记、Root私有settings及PID1 credential移除，Core租约失效。
4. 新产生的两笔任务分别从InFlight变Unknown、Queued变Cancelled；原回执保持，没有增删其他回执。
5. 释放迟到DNS/TLS/HTTP响应后重新读取，任务结果保持。仅重放同一笔OFF请求，精确配置回执仍为代次4，不重发任务。
6. OFF时重新排队启动job/直接启动worker服务，均不创建worker或租约，不改任务结果。
7. 测试父进程收尾后，临时单元与临时polkit规则集合恢复到测试前，没有创建系统账户或停止共享polkit服务。

## 兼容检查

同一来源程序还通过原有完整managed服务协调（含一键合成测试、强制清理额外子进程）、实际多UID IPC、五种独立worker场景（正常、失去ACK、提交后硬退出、错误出口、错误CA）和unmanaged主管检查。Python语法、WSL YAML解析及diff检查通过。只改Python/CI，生产Rust未变，未为本轮重新宣称全仓Rust或ARM验收。

复现入口（Root、systemd、cgroup v2、polkit及隔离环境准备沿用原主管夹具）：

```sh
cargo build -p rustsimhub-core -p rustsimhub-forwarder --features dev-fixtures --locked
for phase in dns tls http late_ack; do
  sudo -n unshare --net python3 tools/test-forwarding-supervisor.py \
    --core target/debug/rustsimhub-core \
    --worker target/debug/rustsimhub-forwarder --managed --close-at "$phase"
done
```

`.github/workflows/rust.yml`已添加相同四阶段运行，不预先把云端配置写成云端成功。私有证据：`.private/forwarding-close-build-20261102-second.json`、`.private/forwarding-close-runtime-20261102-first.json`、`.private/forwarding-close-compat-20261102-first.json`。

## 边界与接续

本轮不访问棒子/NAS，不发送真实短信/推送，不启普通蜂窝数据，不改变设备Root服务或应用71。回环中的合成消息不是运营商短信，系统安装、真实网卡、关闭故障、实际设备冷启动仍未验证；迟到204发生在关闭已确认之后，不冒称覆盖所有响应与撤权的调度交错。

FWD-13只记录部分实际进程证据，保持进行中，不标完整验收。原P0–P7 **81.5%**、行动台账 **20/169**、加权 **10.6%**及主线代码约 **90%**未加权粗估保持。继续主线集成，人工测试最后。
