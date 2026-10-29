# D-22局部实现：有界Unix运输与失效处理

2026-10-30，接续[IPC提案](recovery-ipc-proposal.md) / PR #88。
用户继续推进后先交独立离线实现；随后已回读#88合并并正常接续最新main，未安装或启用生产端点。

后续局部交付：[验证工作保留与迟到清理](recovery-verifier-lifecycle.md)提供独立线程执行器；
以下是 #92 原交付截面，合法跨 UID 准入、A 会话和生产接线仍待完成。

## 实际代码与默认边界

`crates/core`增加一个明确的库目标`src/lib.rs`，只在Linux且显式
`recovery-transport` feature下导出`recovery_transport`。
现有Core二进制没有调用此库、没有新CLI/监听器/角色配置或Wi-Fi派发。
不增加workspace成员、依赖或锁文件，不修改Auth/Store/密码协议/Root材料。
添加库目标是为了独立验证Core运输组件，不创建另一Core进程或业务权威。

| 类型 | 已实现 | 不代表什么 |
|---|---|---|
| Roles | 只接受Core/Edge/forwarder/network/BLE五个非0、非uid_t(-1)、互不相同的角色；Core UID来自当前进程 | 不查询NSS，不分配账户，不批准安装合同 |
| SourceGuard | 接受真实UnixStream后先核SO_PEERCRED，只有配置BLE UID通过；每个Guard最多一个持有中的PeerConnection | 不是恢复公钥/安全会话证明，不是全进程全局连接注册器 |
| PeerContext | 私有字段，只读内核UID/GID/PID及本次CSPRNG连接nonce，无Clone/Deserialize/Debug | 不是Owner、grant、Principal或可执行业务的能力 |
| Packet | 有界opaque数据、无Debug/Serialize，正文用Zeroizing持有，拒绝超限构造和带数据的Close | EncryptedRequest只是运输tag，没有声称已经认证解密 |
| FramedIo | 真实Unix字节流的严格读写、单次绝对截止、取消/错误后永久fence | 其公开构造只提供编解码，不提供任何来源/业务授权 |
| PeerConnection | 拥有通过SourceGuard的原始描述符、上下文与占用槽，无into_inner/Clone；失效后不再给context | 不代替A验证器、Auth实例/realm/grant代次复核或D-23账本 |

SourceGuard在凭据验证后取得单占用槽，再生成非零32字节随机nonce；失败自动释放槽。
PeerConnection失效后仍保留槽，直到宿主丢弃连接；字段析构顺序先关闭流再释放槽。
宿主必须只维护一个Guard，并在外部A验证任务仍需排空时保留连接/占用责任。
这个槽不能证明外部密码任务已经停止。没有A调用/后台任务注入或默认Mock后端。

## 封套与时限

沿用#88候选的IPC格式：4字节big-endian长度，正文`version=1 | kind | opaque`。
kind仅Handshake=1、EncryptedRequest=2、CloseTransport=3；应用信封最大2048，正文最大2050。
长度在2～2050内、版本/kind正确、Close零payload，才分配opaque正文。
不能把通用64KiB Auth解析器当作这一步检查，也不透传普通AuthCommand。
Handshake/EncryptedRequest的密码学最小长度由后续获审A端口判定，本库不臆造套件格式。

每个完整读或写固定5秒绝对截止，包含头、tag和数据；不在每一段重置计时。
开始和返回结果前都核期限，避免已就绪I/O绕过已过期截止。
测试可用私有较短截止检验失败，但产品公开读写没有可由网络输入放大的预算参数。

在第一次await前设置不可复用标记，只在**完整成功且不是Close**时解除。
未poll的future不产生副作用；已poll后取消、EOF/截断、坏头、超时、写失败均保持失效，
后续读写只返回Closed，不尝试流重同步或重发。Close成功也作为终态，不能再发送下一帧。
写失败/取消可能已经写出字节，不能据Error称“没有执行”；后续Core必须保留原业务操作Unknown。
本库不创建/清除业务回执，也不以关闭运输冒充取消Wi-Fi或撤销grant。

SO_PEERCRED是连接建立时的内核凭据，不能证明FD转交后的每一个写入者身份。
Host仍须在同一Core里验证A端到端证明，并绑定当前Auth实例/连接nonce/设备pin/realm/
Owner/grant代次、期限与动作；收到EOF之前的context也不是“对端此刻仍在线”的保证。
这些会话/异步任务管理与真正GATT接线未实现，缺A时生产入口继续unavailable。

## 验证范围

真实UnixStream::pair的14个库测试：边界/连续帧/逐段拆包、全部截断点、坏长度/版本/kind、
Close数据拒绝、固定截止、已就绪但过期、future未poll/已poll取消、真实写缓冲阻塞、写失败、
终态、角色别名/Root/sentinel拒绝，以及真实内核peer先拒绝而不解码恶意长度。
编解码正例直接测试**没有授权含义**的FramedIo；未用假UID/手造PeerContext冒充准入成功。
发送取消夹具先等实际writable，再把有界Unix发送缓冲填到WouldBlock，验证真正阻塞的future。

本轮以普通UID运行，未执行Root身份切换；不同UID的合法SourceGuard准入、NSS/组继承、
随机nonce与占用槽在该真实正例中的生命周期仍需隔离Root夹具复核，明确未运行。
本批不是A密码安全、合法BLE会话、完整D-22、Wi-Fi恢复或设备验收。
完整命令、精确HEAD与实测结果见[交付报告](../deliveries/member-d-recovery-transport.md)。

## 接续与回退

前置#88已实际合并，本PR直接到main；合并事实不等于A密码套件或生产安装已获批准。
后续由同一Core的可信宿主把SourceGuard与SocketRuntime、获审A验证器及#61私有能力语义接在一起，
验证只在独立有界工作项执行，前后重新核当前授权，迟到结果不能重新成为有效请求。
不更改#84授权格式、不直接引入旧#60/#63迁移，D-23业务执行账本另审。
生产未调用/未迁移，回退本feature/库即可，无设备或数据恢复动作。
