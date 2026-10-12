# 所有者恢复、客户端 CLI 与 core 生命周期

本轮在 P1 公钥认证基础上补齐离线 owner 恢复、身份私有备份、Linux simhubctl 和正常关闭/重启。
仍没有电话/短信发送 API、HTTPS、网页密码或可刷写发行版。

## 从旧备份恢复授权

Store::restore 仍只恢复到新的私有目录，轮换 db_epoch 并隔离短信不确定操作及全部授权。
恢复不会自动登录，也不会开启首次所有权初始化。

`--recover-owner` 是独立离线命令，需先停止 daemon；Store 单写者锁阻止在线执行。
只接受已存在、inactive 的原 owner 公钥，不能换公钥、换角色或恢复 client/nas。
设备 challenge 的 purpose=recover_owner，绑定 owner 公钥、设备公钥、随机进程 instance、
一次 nonce、ALL_SCOPES 和恢复库的 generation。原 owner 需要其私钥签名证明。
store 在事务内再次检查同一 inactive owner 和同一 generation，再 active=true / generation++。
失败不确认、不留下有效授权，成功后清空内存会话/挑战/配对窗口。

owner 恢复不会启用任何客户端。客户端需在 owner 新开的窗口中重新证明并被明确批准。
已存在 inactive 公钥可以复用原 principal ID；批准时再次核对签名请求的 generation，
更新权限/类型并 generation++。旧会话永远不随重新批准复活。
更改请求公钥、kind、scopes 或期间再次撤销会让批准失败。
这里是明确重授权，不是恢复数据库时自动续权；owner 仍不能通过网络替换/撤销。

恢复库和原库共存仅用于离线检查。部署方必须选定唯一活动目录，不能让两份业务库同时操作 modem。
尚无任意 owner 换人、忘记私钥后的在线重置或恢复出厂功能。

## 设备身份与独立备份

`Auth::open` 先查看 DB 是否绑定身份。已绑定时只 load 既有 seed，缺失立即拒绝，
不产生新 seed 来替代丢失的身份。首次无绑定时才可生成设备身份。

```sh
# source 必须已有有效身份；backup 目录提前由同一 UID 建立，0700。
rustsimhub-core --backup-identity IDENTITY_DIR PRIVATE_IDENTITY_BACKUP_DIR
```

Identity::backup 保存同一私钥到独立 identity.seed，包含格式标记和校验值，0600、fsync 文件/目录，
拒绝覆盖；部分副本会被 load 检出并保留。SQLite 通用备份仍不包含 seed。
这是受文件权限保护的明文私有备份，尚未实现加密导出/异机托管。
只备份本设备身份，不允许拿其他棒子的身份/NV/校准材料覆盖本机。
owner 的客户端私钥也必须由 owner 保管，不能从设备 DB 反推出或生成替代私钥。
完整硬件恢复备份/系统恢复入口仍属于未闭环的 P0。

## Linux simhubctl

新 crate 为 rustsimhub-ctl，二进制名 simhubctl。仅使用本机 Unix socket，未接入 HTTPS 或远程网络。
私有客户端目录 0700，client.seed/锁/会话文件 0600，由当前 UID 拥有，拒绝链接。
client.seed 使用独立 RSHCLI01 格式及随机私钥，不复用设备 RSHKEY01。
load 不会在丢失时自动生成新客户端，keygen 是明确创建入口。

```sh
simhubctl --help
simhubctl keygen PRIVATE_CLIENT_KEY_DIR
simhubctl sign PRIVATE_CLIENT_KEY_DIR DEVICE_PUBLIC_KEY_HEX bootstrap
simhubctl sign PRIVATE_CLIENT_KEY_DIR DEVICE_PUBLIC_KEY_HEX recover_owner
simhubctl sign PRIVATE_CLIENT_KEY_DIR DEVICE_PUBLIC_KEY_HEX login
```

sign 从有界 stdin 读取 challenge JSON，验证固定设备公钥签名及全部字段，然后签 exact transcript。
它检查 domain、purpose、双方公钥、nonce/instance、kind、scope、generation；不提供任意消息签名。
DEVICE_PUBLIC_KEY_HEX 必须经可信离线设置核实，不采用静默首次信任。
输出是签名字节数组；交给同一个仍在等待证明的离线 core 进程，并结束 stdin。

```sh
rustsimhub-core --provision-owner STATE_DIR IDENTITY_DIR OWNER_PUBLIC_KEY_HEX
rustsimhub-core --recover-owner RESTORED_STATE_DIR IDENTITY_DIR OWNER_PUBLIC_KEY_HEX
```

两条命令先输出 challenge 一行，再等有界签名字节数组及 EOF，60 秒后签名无效。
不要把 core 的双向交互简单接成单向管道；开发烟测用私有 FIFO + JSONL 文件协调。
这些操作需可信维护登录；网络 IPC 没有 provision/recover 命令。

```sh
# 会话目录需提前创建为 0700；新会话文件不能覆盖已有文件。
simhubctl login SOCKET PRIVATE_CLIENT_KEY_DIR DEVICE_PUBLIC_KEY_HEX NEW_SESSION_FILE
simhubctl status SOCKET SESSION_FILE
simhubctl capabilities SOCKET SESSION_FILE
simhubctl open-pairing SOCKET OWNER_SESSION_FILE
simhubctl pending SOCKET OWNER_SESSION_FILE
simhubctl clients SOCKET OWNER_SESSION_FILE
simhubctl pair SOCKET PRIVATE_CLIENT_KEY_DIR DEVICE_PUBLIC_KEY_HEX nas status.read
simhubctl approve SOCKET OWNER_SESSION_FILE PAIRING_ID CLIENT_PUBLIC_KEY_HEX nas status.read
simhubctl revoke SOCKET OWNER_SESSION_FILE CLIENT_PUBLIC_KEY_HEX
```

pair 可用 client 或 nas，scopes 为逗号分隔的明确列表，没有 wildcard/all。
pair 输出 ID、公钥和权限，owner 的 pending 也显示同一请求，批准时必须显式传入精确内容。
pending/clients 仅原 owner 且最近 5 分钟证明的会话可用，不让 NAS 通过 clients.manage 冒充 owner。
login 会核实设备 pin 并将 token 保存到独占新建的私有会话文件，stdout 只有成功/到期信息。
会话记录包含设备/客户端公钥和 daemon UID；后续请求核对本机 UID 与受保护 socket 目录，
不能将它用于共享可写目录或不可信远端。这不是 TLS 通道，不能抵御同 UID/root 已控制主机。
会话字段严限长、无 Debug，持有 token 的缓冲尽量 zeroize；不宣称清空全进程内存。
token 15 分钟后或 core 重启/撤销后失效，重新 login 到新文件即可；不在 argv 中传 token。
keygen/sign 的 Linux 权限边界已验证，Windows ACL/二进制交付未验证。

## 安全关闭与 socket 重启

鉴权 daemon 在接受请求前持有专用运行目录中每 socket 的 0600 文件锁。
父目录必须为服务 UID 拥有、非链接、组/其他不可写；锁拒绝链接、硬链接和宽松权限。
最多跟踪 8 个请求，SIGTERM/SIGINT 优先停止接入；已接受请求最多等待 16 秒，然后取消异步任务。
SQLite 的已排队任务在 blocking worker 中关闭，最后释放数据库锁与 socket 锁。
清理只删除自身 dev/inode 的 socket，不删除已被替换的普通文件或其他 inode。

正常退出后可使用同一 socket 路径重启。崩溃留下的 socket 只有在取得独占锁、
确认由当前 UID 拥有且为 socket、非阻塞 connect 明确返回 ConnectionRefused、
再次确认 inode 未改变后才清理。活跃的旧版 listener、连接队列拥塞、权限错误或未知结果都拒绝接管。
参考：[rustix connect](https://docs.rs/rustix/1.1.5/rustix/net/fn.connect.html)、
[Tokio JoinSet](https://docs.rs/tokio/1.53.2/tokio/task/struct.JoinSet.html)。
该新生命周期只用于 --serve-authenticated，v1 只读诊断保持原行为。
依赖同 UID 维护方不篡改锁文件；不抵御同 UID/root 替换私有目录或并发改写文件系统。
极端磁盘 I/O 卡死不能靠 Rust cancel 终止；生产 systemd 仍需 TimeoutStopSec/watchdog 和恢复策略。
开发烟测使用 TimeoutStopSec=30 / RuntimeMaxSec=180，尚未安装开机服务。

## 验证与下一步

失败路径包括假 owner/假签名/重放/错 purpose、generation 变化、DB 恢复提交失败、
客户端重新批准前不能登录、旧会话不复活、错 device pin、seed 跨角色和缺失拒绝、
私有会话不覆盖/不跟随链接、live/stale/replaced socket 和两次同路径关闭重启。
实机烟测入口为 tools/device-recovery-cli-smoke.sh；只用随机私有目录中的合成身份与数据库。
参数和原始资料不进入 Git，完整实测记录见 VALIDATION-RECOVERY-CLI-P1.md。
下一包接入受限业务 IPC 与 MM SIM/短信事件、观察期和真实对象对账，再推进实际发送与通话。
