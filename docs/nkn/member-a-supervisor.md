# 成员 A：独立 NKN 监督状态机候选实现

对应 A-25；供负责人审核，尚未接入 Core、Root 或真实 NKN SDK。

`crates/nkn-agent` 是无第三方依赖、禁止 unsafe 的实际 Rust 状态机，
不是启动脚本，也不是用 Mock 结果冒充的引擎。测试仅替换 Root、引擎和时钟边界。
生产默认 `UnavailableRoot` / `UnavailableEngine` 无法启动任何东西。

## 生命周期与引用

1. 构造 `Supervisor` 不读时钟、不访问 Root、不创建线程、子进程或网络。
2. `acquire(Owner)` 最多为手动引擎、远程入口、统计各分配一个引用。
   返回的 `Reference` 私有字段、不可复制、不能序列化；它只是本地所有权票据，
   不携带鉴权。宿主必须显式 `release(reference)`；丢弃票据不等于释放。
3. 宿主先 `arm()` 取得单次运行的 `Cancellation`，再调度
   `begin(context, policy, proof)`。提前取消、重复调用、旧上下文都不能到达 Root。
   `arm` 不授予权限，`policy` / `proof` 的类型与验证由真正的 Root 提供者拥有。
4. 原始证明在调用 Root 前保存。Root 明确拒绝且没有副作用才退回 Disabled；
   回执不明、超时、部分启动都保留原证明，进入清理流程。
5. Root Active 后才调用引擎启动。仅创建进程得到 Starting；只有真正引擎适配器
   观察到 SDK Ready 才能报告 Ready。PID、stdio 前端的 unknown 状态都不足以证明就绪。
6. 每个 `drive()` 推进有限步骤，不创建后台重试循环。统计引用释放不会关闭仍由
   远程入口持有的引擎；最后一个引用释放或显式取消后只清理当前运行。

旧实例/旧引用不能释放新实例的资源；旧取消句柄不能取消下一轮运行。
已经取消的运行不能被新加入的引用救活，必须先证明原资源释放再显式开始下一轮。
计数器耗尽返回错误，不回绕。快照只有有限状态、计数和错误码，不含证明、路径、PID 或秘密。

## 有界启动、重试与停止

| 路径 | 实际约束 |
| --- | --- |
| 初次启动 | Root 与首次引擎启动共享最多 10 秒，且受原证明到期时间限制 |
| Starting 观察 | 沿用该次启动截止时间，重复轮询不能延长 |
| 断线重试 | 只在明确没有子进程的 Offline，或观察到原子进程 Exited 后进行 |
| 重试预算 | 原运行最多 3 次启动、间隔至少 5 秒、总窗口最多 30 秒；不重新启动 Root、不续期 |
| 永久错误/不明 | Denied、Unavailable、Protocol、Deadline、部分进程、未知句柄不自动重试 |
| 停止 | 子进程停止与 Root 停止共享总计最多 5 秒；多次 drive 不延长 |
| 停止回执不明 | 已发送的 stop 不重复发送，只查询原资源 |
| 停止窗口到期 | 只读核验原子进程与 Root；不能借轮询补发尚未发出的 stop |
| 显式继续清理 | `resume_cleanup()` 只允许当前未解决的原范围获得一个新停止窗口；不重发已经发出的 stop、不启动或续期 |

清理首先观察原子进程已退出并回收，再清理原 Root 范围。两项都被证实后才回到
Disabled；单独收到 Root Released 不能丢弃仍未证明退出的进程句柄。
已过时的成功回执不能直接消除 Unknown，必须重新查询。超时后的只读核验使用
独立最多 5 秒查询预算；这是查询期限，不是自动延长停止期限。

`Limits::bounded` 只能缩短启动/停止/窗口、减少次数，或增加重试间隔，
不能放宽默认上界。单调时钟倒退保留原资源并报错。

## 真实适配器必须满足的契约

这份库没有定义新的 Root 消息格式、租约签名、路由参数、UID 或 shell 入口。
具体接口提案见 PR #129 的 `member-a-root-routing-proposal.md`；尚待负责人审核。
此库的 trait 是进程内边界，不代表已经冻结的生产 IPC。

- Root 必须验证真实 peer credentials、原租约、有限策略和到期时间，原子拒绝已被
  其他所有者占用的注册。`Rejected` 表示确定无副作用，不能用于丢失回执。
- Root `Released` 必须证明原范围没有策略、子进程和能够晚到重建资源的路径。
  不能仅凭请求已发送、PID 字符串或任意布尔值返回成功。
- 引擎提供者必须只管理自己持有的子进程；`Exited` 代表真正观察并回收原子进程，
  `Ready` 代表真实 SDK 就绪，不能把 stdio 前端可用解释为 NKN 网络可用。
- 每个提供者必须在副作用之前检查 `Budget`，按剩余时间设置真实 IPC/I/O 截止，
  启动/状态回调中检查取消。库会复查迟到结果，但无法抢占阻塞或恶意的同步实现。
- 清理预算不因启动取消而取消；这不会绕过 Root 的原证明验证。
  到期证明被拒绝清理时保持 Unknown，依靠 Root 独立的租约到期回收。
- `Drop` 只对原范围做有界的尽力清理，不能代替宿主持续 drive、Root 独立回收或
  崩溃后的真实核验。不能宣称 drop 就证明了无残留进程。

## 验证边界

31 项单元测试运行真实 Supervisor 代码，覆盖默认关闭、三引用、旧票据、取消、
迟到/丢失回执、部分启动、启动与停止总预算、重试次数/间隔/窗口、原范围清理、
时钟倒退和不影响其他所有者。合成边界不证明实际低权限 UID、cgroup、Root
租约、设备重启恢复或 NKN 连接；这些仍需真正适配器与隔离/实机验收。

```sh
cargo test -p rustsimhub-nkn-agent --locked
cargo clippy -p rustsimhub-nkn-agent --all-targets --locked -- -D warnings
cargo check -p rustsimhub-nkn-agent --no-default-features --locked
```

没有修改 Core 的生产接线、NKN 冻结字段、BLE/Noise、Root 权限或密码学冻结标记。
本候选交付不能将 A-25 整项或全项目标记为已验收。
