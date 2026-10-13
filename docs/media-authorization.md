# P3B 核心与媒体授权通道

本阶段接通受限本地媒体授权，仍未实现 Opus/WebRTC 或手机声音链路。
`media_available` 保持 false；`call_status.media_authorization_api` 单独表示授权通道已启用。

## 启动与接口

默认 `--serve-telephony` 不发媒体票据。新增显式模式：

```text
rustsimhub-core --serve-media-authorizing CONTROL_SOCKET CONTROL_UID STATE_DIR IDENTITY_DIR MEDIA_SOCKET MEDIA_UID
simhubctl issue-media CONTROL_SOCKET SESSION_FILE PRIVATE_LEASE_FILE PEER_PUBLIC_KEY_HEX NEW_PRIVATE_TICKET_FILE
rustsimhub-audio-alsa --watch-authorized MEDIA_SOCKET CORE_UID PRIVATE_TICKET_FILE SECONDS
rustsimhub-audio-alsa --duplex-authorized MEDIA_SOCKET CORE_UID PRIVATE_TICKET_FILE SECONDS
```

新 core 模式同时运行既有显式通话/短信控制，所有写操作仍走原有 scope 和持久提交门禁；启动它不会自动发送或拨号。
`issue-media` 只把票据写入新的私有文件，不把凭据放入 argv 或正常 stdout。
票据文件是开发诊断交付方式；正式 edge/WebRTC 的安全交付接口还没有实现。
原生诊断时长限制 1—60 秒。watch 不打开 PCM；duplex 只有授权成功才启动 PCM，
采集帧只计算统计并丢弃，上行只送数字静音，没有录音文件或网络声音输出。

控制 socket 采用现有登录 token、UID 限制与 65,536 字节协议；媒体 socket 独立锁与生命周期，
只允许配置的 MEDIA_UID，最多两个连接，每帧不超过 2,048 字节。
媒体协议 v1 只有首帧 redeem(ticket, peer_key) 和后续 pulse，没有任意命令、scope/role/asserted ACTIVE、拨号或短信接口。
每个连接请求 ID 必须从 1 顺序递增；只能 redeem 一次。连接关闭自动取消该连接的授权，不取消其他连接。
两个 socket 的父目录必须由 core UID 拥有，禁止 group/other 可写；媒体客户端还验证路径、父目录和 SO_PEERCRED 的 CORE_UID。
配置跨 UID 时，媒体 socket 用专用共享组的可遍历目录/660 socket，控制 socket 和 state/identity 继续保持私有。
应用不会修改用户/组或自动放宽文件权限。生产 systemd/UID/GID 安装配置仍需单独验证。

## 真实核心快照

IssueMedia 必须同时具有 media.use 与 calls.control，且两次 authorize 返回完全相同的当前 Principal。
caller 必须持有当前独占 call lease；客户端上报的剩余秒数不改变服务端截止时间。
核心从自身状态生成 Context，不接收远端 Context JSON：boot、backend/owner/connection fence、SIM line epoch、
call UUID/incarnation、租约 generation、principal UUID/generation、peer_key 与保存的期限。
每次兑换和 pulse 都重新检查实际 MM 呼叫 State=4、号码/方向未变、当前身份/注册状态、对象路径 fence 和持久 Principal。
Hold、结束、对象替换、监听器失效、MM/连接/SIM 变化、撤销或代次变化都拒绝续验。
发行票据后服务器只保存票据摘要；票据最长 5 秒且单次消费。媒体进程不会拿到客户端控制 token 或 signing seed。
客户端可通过登录后的控制请求指定 peer_key；原生文件诊断只证明票据交付和进程身份。
这 **不是** 手机端对该 key 的密码学持有证明；真正 WebRTC 适配器仍须核对 DTLS 对端/协商绑定，不能用传入 key 字段代替证明。

## 截止时间、撤销与故障

core 控制租约和媒体进程改用同机 Linux CLOCK_BOOTTIME 毫秒，包含 suspend 时间；不比较两个进程各自的 Instant 起点。
参考 [rustix clock_gettime](https://docs.rs/rustix/latest/rustix/time/fn.clock_gettime.html)。
票据兑换后 core 返回 boot 与绝对 until_ms；每次只续最多 1 秒，并受到真实控制租约截止时间上限约束。
媒体客户端每 250ms pulse，单次请求总超时 500ms；核心媒体队列容量 4，调用等待 450ms。
协议/UID/boot/序号/窗口异常、I/O 超时或 EOF 立即 revoke；失效或过期的镜像授权不能续活。
即使 core 停止响应，PCM 每步检查自己的绝对截止时间，最迟在一秒窗口结束后停止。
MM/property/SQLite 慢查询可能使音频提前失败；没有为“继续有声”而绕过门禁。
媒体快照目前由通话 actor 串行取得，优先挂断仍排在媒体队列之前；D-Bus 延迟与 CPU 成本需实机测量后优化。

手动 Hangup、控制租约清理和正常 core 退出先撤销媒体；媒体断开只清理媒体授权，通话仍按原有 30 秒控制租约处理。
媒体过期状态在下一次请求时回收，避免丢失 disconnect 通知后永久占用槽位。
新的清理检查会先核对 MM 号码/方向，拒绝因路径复用而误挂另一通话；没有信号且相同路径/号码/方向的歧义仍不能完全消除。

ALSA I/O 在独立 std 线程，控制授权在 Tokio 线程；媒体退出先 drop 客户端撤销 Permit，再关闭/等待 PCM 线程。
nonblocking I/O 避免正常 read/write 等待，但异常驱动 ioctl/open/prepare/close 的最坏阻塞仍未证明；
实机诊断单元设置 RuntimeMax 和短 TimeoutStop，作为进程退出兜底，不能把它当驱动实时性已验收。
授权失效后队列清空、禁止 drain 旧声音；日志只包含有限错误与帧数/峰值/xrun。

## 验证与后续

核心集成测试使用私有 D-Bus fake service 和真实 UDS：实际 owner/scope/lease、held、重放、错误 UID、畸形帧、
撤销、号码改变、binding 丢失、连接关闭、socket 清理。媒体组件测试私有票据权限、symlink/hardlink 拒绝和过期不复活。
真实系统记录见 VALIDATION-MEDIA-P3B.md；不以模拟和 null PCM 测试声称双向通话成功。

后续重点是实际 PCM 参数/释放、授权服务独立负载与生命周期优化、媒体状态/事件、WebRTC/Opus/DTLS/ICE/TURN、
真实上行听音及来电/DTMF。完整系统镜像、冷启动路由初始化与系统恢复仍未完成。
