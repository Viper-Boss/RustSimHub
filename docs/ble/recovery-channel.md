# D-22：拥有型运输/授权桥与无锁断连失效

2026-10-30，接续 #97 的私有能力和 #95 的独立验证线程。
默认不启用；仅 Linux 同时显式 recovery-transport/recovery-grants 导出 Core 库模块。
没有新监听器、验证器配置、密码协议或业务派发。

## 句柄本身负责失效

Auth 的 RecoveryConnection 现在拥有私有共享原子 live 标记；invalidate/Drop 不取得 Auth
mutex。内部 Connection 在配置替换/关闭/Auth 释放时也将同一标记变 false，不能重新激活。
prepare、finish 和 capability check 在数据库复核前后检查存活；运输句柄已释放即拒绝旧能力。
下次 trusted open 仅在没有工作/结果仍占用 Permit 后清除旧连接记录，生成全新标记及 nonce。
旧句柄迟到 Drop 不会修改新连接；这不是 grant 撤销或 Wi-Fi 回滚/清理完成证明。

旧代码在 drop(token) 后仍可通过旧能力检查，回归实际失败；新代码在已持有 Auth mutex
时 drop(token) 也可失效，不死锁。标记是在本地释放/失效时观察的状态，不证明远端当前在线。
任意检查返回后的并发关闭仍须下一业务 gate 复核，不是跨 await 的不可撤销授权租约。

## Core 拥有型桥接

RecoveryChannel 的唯一构造输入是 SourceGuard 接纳的私有 PeerConnection 和同一个 Auth。
它内部读取 peer nonce，不接受 UID/channel/nonce 的网络字段，也不提供原始流构造器。
字段顺序先丢弃 Auth token 再关闭 peer/释放运输槽；无 Clone/Serde 或普通 Principal 转换。

```text
同步短 Auth 阶段 attach / prepare
    → 无 Auth 借用的 PendingVerification.verify().await
    → 同步短 Auth 阶段 VerifiedChannel.finish / 后续 capability check
```

宿主沿用自己的 Auth worker/锁执行同步阶段，释放锁后才 poll verify。
桥不内建第二套 Auth mutex/线程池；#95 的线程拥有 peer/packet/工作，五秒等待超时或取消
后继续保留真实资源。Auth token 留在等待方，调用 future 被丢弃后失效，旧工作仍须排空。
VerifiedChannel 在 Drop 开始先失效 token，再析构结果/Permit，最后关闭 peer/释放槽。
finish 失败也关闭本 channel；成功只交回同 channel 和经 Auth 复核的内部请求，不执行业务。

read/write 同样消耗 channel；丢弃未 poll 的拥有型 future 也会关闭 channel。
它与借用型 FramedIo.read/write 的“未 poll 无副作用”不同，API 文档明确区分。
完整 CloseTransport 使 token 失效；普通 EOF/错误/已 poll 取消均随拥有型值释放而关闭。
prepare 仅接受 EncryptedRequest tag，该 tag 本身不是密码成功证明，实际验证仍交给 A。
Handshake 可作为有界运输帧读取/写入，但本桥没有 A 握手流程或默认 Mock 成功路径。

## 真实证据与限制

新增5项 Auth 生命周期测试调用实际 Auth/Store：Drop 时已持有 Auth mutex、重复 invalidate、
待排空工作/未消费结果保持 Busy、旧 token 不伤新连接、配置/Auth Drop、真实阻塞 verifier。
只替换外部 verifier，不伪造 SourceGuard 的合法 peer。初始 Drop 回归实际退出101后修正通过。
既有 Auth/Store、运输24项和整仓回归继续核对，详见[报告](../deliveries/member-d-recovery-channel.md)。

当前 UID1000，无 newuidmap/合法不同 UID 的可执行环境。因此 Core 正向 attach/read/write/
prepare/verify/finish 的真实跨 UID 全链路**未运行**；本桥只经过编译/严格 Clippy及组件回归。
没有构造假的 PeerContext/PeerConnection 来填补该正例。此限制阻止完整 D-22 验收，
不以 Auth 的合成 transport nonce 测试冒充内核来源验收。
A 真实安全协议/互操作、GATT/BlueZ、安装角色/权限、D-23执行及设备仍未完成。
release panic=abort、普通 panic hook 和永久卡住线程需占用的边界保持，未宣称进程隔离。

无新依赖、wire/schema、安装器或默认启动改动；没有设备/生产数据/业务网络操作。
回退桥接/失效补丁不需数据迁移。沿用 AGPL-3.0-only/既有库；AI 辅助情况见报告。
