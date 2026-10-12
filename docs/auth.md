# 设备身份、配对与鉴权：当前实现

新增 auth / policy crate，运行于 core。只支持客户端公钥认证与受限本机 IPC，
尚无网页密码、HTTPS、远程端到端通道或真实电话/短信写接口。
系统登录用户、owner、client、nas 相互独立，USB 接入不自动建立所有权。

## 身份与持久化

Identity::open 使用 OS 随机源生成 32 字节 Ed25519 seed。
Linux 身份目录由当前 UID 拥有且无组/其他权限，文件 0600，拒绝符号链接和硬链接。
文件锁串行化首次创建；先写 identity.pending、校验、fsync，再 rename 和 fsync 目录。
完整 pending 可继续使用，部分/损坏文件保留并拒绝，不自动重建。
SigningKey 与 seed 缓冲使用 zeroize；Identity/Token/私有请求不实现 Debug。
这不等同于全进程内存清零、加密存储或抵御 root 读取。

SQLite schema v2 增加 device_identity 与 principals。从 v1 原子迁移，保留原操作、事件及 cursor。
公钥必须与独立 seed 文件一致；不匹配拒绝启动，不能自动重绑。
设备公钥与 SIM、MAC、IP 无关。SQLite 备份不含私钥，身份需要独立私有备份流程。
principals 保存随机 ID、唯一公钥、kind、scope 位图、递增 generation、active。
最多 64 条含已撤销记录；owner 唯一，不允许网络替换或撤销。
owner 建立、批准、撤销经过 SQLite 单写者；提交失败不返回成功。
storage 身份写方法是 core 内部接口，网络进程不能取得 Store handle。

## 所有权与配对

首次 owner 由可信离线命令提供的公钥建立，要求其私钥对设备 challenge 签名。
IPC 没有 bootstrap 命令；SSH UID 本身不是 owner。
交付时仍需可信设置工具、指纹展示、设备带外核实和用户引导。

设备返回签名及 EXACT transcript 字节数组。客户端先验证签名及字段，再签原始字节，
不能重新序列化 JSON。字段为 domain=RustSimHub.Auth.v1、purpose、进程随机 instance、
32 字节随机 challenge_id、device/client 公钥、kind、scope 位图及持久 generation。
instance 比 Linux boot_id 更严格：只重启 core 也会失效。
使用 ed25519-dalek 2.2.0、拒绝 weak key、verify_strict 验证。
参考：[VerifyingKey 官方说明](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html)、
[getrandom OS 随机源](https://docs.rs/getrandom/0.4.3/getrandom/fn.fill.html)。
这只承担授权证明，未来远端须另行使用成熟 TLS/Noise 传输加密。

挑战 60 秒，一次使用；错误签名也消耗挑战，bootstrap / pair / login purpose 隔离。
最多 32 个挑战；签名/挑战共享令牌桶：初始 32，每秒补充 8，最多 32。
这里只限制本机资源，网络入口的来源/账号限速及远程 DoS 防御尚未实现。

owner 以最近 5 分钟证明的会话开启 5 分钟窗口。
client/nas 申请公钥、kind、精确 scopes，签名后进入待批准状态，尚不能登录。
owner approve 明确同一公钥、kind、scopes，任意改动拒绝；最多 16 个待批准请求。
重新开窗清除旧挑战/请求；到期、重启后旧请求失效。
当前无待批准列表 UI；工具需可靠展示 ID、公钥和权限，不能只展示可伪造名称。

## 会话与权限

已授权 active 公钥才能领取 login challenge；证明后生成随机 256 位 opaque token，
返回一次，内存只保存 SHA-256 摘要，最多 64 会话，15 分钟绝对到期。
期限使用服务内部 Instant，无客户端时间戳和墙上时钟输入。
重启后 token、challenge、pairing、窗口失效，长期授权仍在设备；无需 NAS 续签。
每次 authorize 从 DB 校验 active、generation、kind、scopes 与会话快照。
撤销先提交 generation++ / active=false，再移除会话/挑战；其他 Auth 实例也会拒绝旧记录。
未来硬件写操作必须在最终执行点与撤销串行化，不能只在网络入口检查。

| bit | scope |
|---|---|
| 0 | status.read |
| 1 | sms.read |
| 2 | sms.send |
| 3 | calls.read |
| 4 | calls.control |
| 5 | media.use |
| 6 | network.manage |
| 7 | clients.manage |
| 8 | update.manage |
| 9 | system.manage |

位编号固定；空、重复、未知 scope 拒绝。
NAS 即使获 clients.manage 也不是 owner，不能开窗、批准或撤销其他用户。
只实际消费 status.read 与 owner；其他位为未来契约，不代表业务已实现。

## IPC v2 与 CLI

4 字节大端长度 + JSON，1–65536 字节，单连接单请求，最多 8 连接，5 秒 I/O、15 秒总限时。
SO_PEERCRED 限定一个本机 UID，业务仍需 token。鉴权/DB 在 blocking worker 执行；
Auth 使用 try_lock，竞争即拒绝。Store 与正常 daemon 生命周期绑定，文件锁阻止第二个 writer。
graceful shutdown / stale socket 安全清理尚未完成，重启需确认旧进程结束并使用新运行目录。
错误不含 token、原始请求或私有数据库内容。

```sh
rustsimhub-core --serve-authenticated SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR
# stdin 有界 JSON，token 不放入命令参数或日志。
rustsimhub-core --request-auth SOCKET
# 仅离线，daemon 必须停止；输出 challenge 后 60 秒内输入签名字节数组并结束 stdin。
rustsimhub-core --provision-owner STATE_DIR IDENTITY_DIR CLIENT_PUBLIC_KEY_HEX
```

```json
{"protocol":2,"id":"r1","command":{"op":"identity"}}
```

命令为 identity、challenge、authenticate、health、capabilities、open_pairing、
pair_challenge、pair_proof、approve、revoke，字段见 crates/wire/src/auth.rs。
public_key 是 32 字节数组，signature 是 64 字节数组；challenge_id/pairing_id 是 64 位十六进制字符串。
不能注入 role、shell 或未知命令；v1 请求在此 endpoint 不会降级接受。
旧 --serve-readonly 单独保留为 health/capabilities 诊断，不能作为业务入口部署。

## 恢复与未完成项

旧备份可能早于撤销。恢复事务把全部 principals 设 inactive 并 generation++，
保留唯一 owner 与独立设备身份，隔离提交后才移除恢复标记。
旧备份不能重新获得有效授权，也不能重新打开 bootstrap。
当前缺少恢复 owner 的可信离线重新证明/确认工具；须完成这一工具后再批准需要的客户端。
不能手工删 owner 表来“恢复登录”。owner 变更、恢复出厂及身份备份加密仍需单独规格。

仍需：网页 Argon2id 参数实测、独立 TLS 密钥与 HTTPS/WSS/CSRF；
客户端密钥保管/带外核实/配对列表；SIM 观察期、媒体票据和事件订阅权限；
开机部署、受限硬件权限、关闭重启、完整恢复和断电验证；依赖公告扫描及协议/外部安全审查。
测试通过不能宣称消除漏洞。

合成密钥 fixture 只在 dev-fixtures 构建出现，默认正式构建关闭。不要对生产目录运行 fixture。
实际证据另见 VALIDATION-AUTH-P1.md。
