# 首阶段实现边界

本文件描述已经编写的基础代码；产品级设计见 development-plan.md。

| crate | 职责 | 允许访问 | 当前不承担 |
|---|---|---|---|
| rustsimhub-domain | 通话租约、SIM epoch 隔离、短信提交状态规则 | 调用方传入的身份、时钟和硬件状态 | 鉴权、持久化、真实拨号/短信 |
| rustsimhub-wire | 有界 IPC 编解码及请求契约 | 字节流 | 网络监听、角色授权 |
| rustsimhub-storage | SQLite 单写者、短信幂等/提交、事件、备份恢复 | 专用私有目录 | 鉴权、真实 modem Send、客户端事件订阅 |
| rustsimhub-core | 自检、Linux 只读 D-Bus 探测、只读 UDS 服务及 CLI | system D-Bus、内核版本、UDS | 外网服务、身份配对、数据库、硬件写操作 |

全项目当前 forbid(unsafe_code)。Linux 依赖 zbus 和 Tokio；Windows 运行纯规则与协议测试。
参考：[zbus ObjectManager](https://docs.rs/zbus/latest/zbus/fdo/index.html)、
[Tokio peer_cred](https://docs.rs/tokio/latest/tokio/net/struct.UnixStream.html#method.peer_cred)。

## 通话与短信规则

- 租约有效期 30 秒，使用启动内单调毫秒。boot、SIM epoch、客户身份和 grant generation 一起形成隔离。
- generation 是 fencing number，不是可独立使用的认证凭据。后续 core 必须先校验真实客户端认证。
- core 以自己保留的到期时间判断有效性；客户端提交的 expires_ms 不得延长租约。
- 租约过期不等于基带通话结束；modem_busy 仍阻止再次占用。
- SIM epoch 只能前进。实际 SIM 变化监听、隐私化指纹及持久化 epoch 尚待实现。
- 短信 Prepared → Submitting → Submitted。Submitting 必须先落盘，之后才能调用 modem Send。
- 超时或重启将 Submitting 视为 Unknown，禁止自动重发。换卡取消 Prepared 操作。
- domain 中 SmsOperation 是纯规则对象；持久操作由 storage 的唯一约束、事务与恢复实现，详见 storage.md。实际 modem 调用尚未接入。

## 只读 IPC v1

帧头：4 字节大端 u32；后跟 UTF-8 JSON。长度 1–65536 字节，分配前检查。
每个连接只处理一个请求，最多 8 个并发连接；读取/写入分别限时 5 秒，整个处理限时 15 秒。

```json
{"protocol":1,"id":"request-1","command":"health"}
```

命令只有 health 与 capabilities；未知命令、未知字段、非 v1、空或超长 id 均拒绝。
请求不能声明角色、UID 或 shell 参数。服务通过 SO_PEERCRED 校验唯一配置的 UID。
socket 父目录不得为符号链接，必须由服务用户拥有且不能让组/其他用户写入。
服务不会自动删除旧 socket，避免第二个进程抢占正在运行的实例。
socket 模式 0660；部署方必须为允许的本机调用用户配置目录/组访问权限。

capabilities 只输出架构、内核、MM 是否可查询及 modem 状态/能力。
不会输出设备序列号、SIM 标识、号码、短信内容、网络 SSID 或认证材料。
当前读取 GetManagedObjects 后只序列化白名单字段；后续应收敛到类型化代理和受控事件订阅。

## 下一阶段

完成存储验证、身份/配对/授权边界后，才能添加硬件写命令。
只读探测通过不能代表短信、通话、声音或完整系统验收通过。
