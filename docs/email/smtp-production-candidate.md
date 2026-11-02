# SMTP 产品接线候选：outbox 4 / worker 5

2026-11-03，wnfhsj88。承接已合并的 SMTP/TLS 库、EHLO 修复及兼容提案；
用户明确转交 Core/Root/密码/SMTP 依赖的隔离实现。本包提供待 Viper-Boss 审核
的实现，默认关闭；没有安装服务、修改设备、迁移真实数据或向外部邮箱发信。

## 编译与接口

- forwarder：`smtp-production-candidate`；Core：`email-production-candidate`；
  update：`email-production-candidate`。三个特性均不在 default 中。
- 默认程序继续 outbox 3 / worker 4。候选严格使用 outbox 4 / worker 5；
  不协商降级，普通打开旧库会在写入前拒绝，不自动迁移。
- 私有 Destination/DestinationEdit 的 email 字段为 smtp_host、port=465、
  tls=implicit、username、password、mail_from、rcpt_to。仅单个 ASCII 邮箱、
  DNS 主机和 AUTH PLAIN；限制及 TLS 行为沿用原 EmailSenderV1。
- PolicyView 仍只返回 id/kind/credentials_redacted。真实 Core 的 email_revision=1
  才允许页面选择邮件；旧设备不显示邮件选项。账户输入使用密码控件，Keep 保留
  Core 内凭据，浏览器持久记录只有原请求回执元数据。

## 同一队列和原授权

PreparedDeliveryV1 显式区分 HTTPS/SMTP，完整不可变 MIME、信封和凭据均进入
实际持久 claim 及摘要。Claim 额外包含原 Queue 的真实 Binding；Core 仍检查
实际 Auth/Store Owner、realm、代次、当前 SIM line、attempt 和完整 request。
同一原 Connector/BoundRoute、取消通道及单次尝试原则保持。

HTTPS 使用原三个阶段；SMTP 使用唯一顺序：BeforeConnect → BeforeTls →
BeforeSmtpAuth → BeforeSmtpMailFrom → BeforeSmtpRcptTo → BeforeSmtpData →
BeforeSubmit。每阶段均携带原 claim 重新检查。原 EmailSender 执行实际 TLS/SMTP，
适配层不另写协议解析器或自行连网。结果仍为 NotSubmitted / Confirmed / Unknown，
但确认带 Http 或 Smtp 标签。SMTP 只接受最终 DATA 的 250；HTTP 250 不能充当
邮件确认。BeforeSubmit 许可可能已发出时，worker 自报 NotSubmitted 仍记 Unknown；
不会自动重发。确认只表示邮件服务器接受，不证明最终邮箱或手机收到。

Date 来源为可信 Core 准备时钟；短信 timestamp 仅可作为正文数据。历史预览保存
一次准备秒数，提交重新核对同一实际请求时复用该时间；跨秒不会改变已确认摘要。
SMTP 字段、MIME、已知序列化缓冲使用零化容器；获取 claim 改为移动 JSON 值，
不再克隆完整私有 claim。临时回复值在编码后清理字符串。这是缓冲卫生措施，
不是对 allocator、serde 所有失败分支、TLS/SQLite 内部副本或物理内存擦除的证明。

## 显式维护迁移

更新器新增的 `migrate-email4` 是独立、默认未编译的维护入口，不是 stage、启动
或浏览器命令的副作用。调用链为原 Store/维护 snapshot → 签名程序核验 →
固定 forwarder Root 动作 → 实际 Core UID 子进程。生产路径和身份固定：

- 数据：`/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding`，Core UID/GID 986；
- 程序：`/opt/rustsimhub/selected/rustsimhub-forwarder`；
- Root 控制锁：`/run/rustsimhub-forwarding/control.lock`；
- 备份：`/var/lib/rustsimhub/rustsimhub-v1/system/forwarding/email-backups`。

更新器在持有原业务 writer fence 时验证安装信任和 confirmed slot 的签名载荷；
confirmed 同时是当前和回退角色，存在 pending candidate 则拒绝。候选也必须完整
验签，三个角色的实际 Core/worker 元数据必须一致支持 4/5，并符合原业务 schema
和 Root/start-mode。主库只读 snapshot 核对真实设备公钥、realm、当前有效 Owner、
代次及原 SmsRead/SystemManage scopes。无需读取私有 identity seed。

Root 持有原 control.lock，要求 enabled.json 和 settings.json 均不存在，并调用
原 System.empty 检查真实 worker PID/cgroup/job/触发器/同 UID 存活进程。持有实际
outbox writer 锁时复制 Root 0600 备份，fsync 后重新读回逐字节/哈希核对。只释放
outbox 锁供子进程接管，原业务 fence、Root/slot 观察和控制锁跨整个动作保留。

已签名 selected worker 的摘要再次匹配实际选中程序；执行封存 memfd 的固定子入口，
不接受任意命令/数据路径/UID。子进程先核验 stdin 的内核 SO_PEERCRED 为直接 Root
父进程，再解析有界私有许可；实际 real/effective/saved UID/GID 均为 986、附加组为空、
Inh/Prm/Eff/Amb 能力为零、NoNewPrivs=1。固定 30 秒原许可不续期。子进程取得原
writer 锁并重新核对源 SHA；SQLite 写入/日志均由数据 UID 创建，Root 不执行 DML。

完整校验旧格式 3 的 application ID、已知 schema、Binding/Policy、规范 HTTPS
请求和历史计划后，在同一事务中只给原请求加 transport 标签。非规范对象直接拒绝，
不将不能通过后续精确 claim CAS 的请求写成格式 4。所有业务 ID、attempt、代次、
realm/Owner、正文、凭据、游标和去重记录保持；in-flight → Unknown，prepared
历史预览 → expired，其他回执保留。早期无 challenge 的历史计划补新 challenge，
但不恢复 prepared 确认；新私有日期字段设 0，仅用于失效/终态历史记录。

事务提交前子进程再次请求 Root，Root 重查无 worker/启动文件后才给一次确认；
Root EOF、拒绝、许可过期或原源库变化则回滚。提交后确认丢失仍视为不确定，备份
保留，不自动重试或反向迁移。成功后更新器重新取得实际 outbox fence 并核对 4，
重查 Root 材料；不启动 Core/worker、不打开网络、不抹除 Unknown 或替换应用 slot。
普通 admit-stage 在投影格式 4 后遇到旧格式 3 fallback 会拒绝。

## 验证与尚未验收

详细命令、精确实现提交和结果见交付报告。真实 TLS 使用合成 loopback 服务；
真实低权迁移使用原默认格式 3 写入器及候选子进程、封存 exec、内核 UID/peer、
实际 SQLite 提交/回滚和网络关闭的容器。其输出明确 systemd_producer_tested=false：
没有把 Python 私有父通道夹具冒充真实 PID1/签名维护入口或目标设备验收。
原 Root/签名核验函数已有具体接线和测试，但本次未在真实安装的全部维护门禁下
执行新迁移。ARM 运行、真实系统管理器迁移、经授权邮箱实测、负责人兼容/安全
审核与部署均未验收；不更改全局完成率。
