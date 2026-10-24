# P4G 配网热点Root执行程序

2026-10-24。已实现显式启动的Root执行程序，将P4F保护文件租约连接到P4E热点事务、NetworkManager及固定nft隔离。301项工作区Rust测试、严格Clippy、Core生产检查及执行程序/库ARM64构建通过。本轮没有访问棒子，没有安装服务、开热点、切换Wi-Fi、拨号、发短信或启用普通数据。上次确认的业务仍为185ec6e/confirmed43，整体79.5%。

## 已实现的闭环

- 真实Core文件租约：验证同boot、realm、设备公钥摘要、材料摘要、实例、nonce、序号及绝对截止。Root先持久消费唯一操作，再回执Starting，之后才可能操作防火墙或无线；不提供网络请求指定接口、命令或口令的入口。
- Root持有独占Journal锁。五分钟总期限和15秒确认期限均保持；独立轮询可以取消等待中的D-Bus激活，取消后继续等待受限回退，不能通过丢弃Future来假装结束。
- Gate使用Root私有材料原件和真实HTTPS。先连接固定loopback:8443，以172.31.85.1为TLS校验名称，保留有效期/证书链/SAN验证，并核对精确叶证书；发送固定AP Host、Origin和现有API标记，通过新的随机请求ID核对真实Core公钥及协议。
- 初始loopback检查不会当作AP网页可达。NetworkManager显示Ready后，再通过实际172.31.85.1:8443验证同一TLS/身份请求，成功才向Core报告Ready。启动阶段允许最多10秒等待Edge绑定地址；Ready后失去网页可达会取消回退。
- 两条HTTPS检查分别最多缓存4秒，材料文件每次均核验，避免持续探测耗尽网页的有界请求额度；没有增加已有Edge限额或绕过证书验证。
- 关闭顺序保持：回退精确候选并验证，再释放自己创建的隔离表，最后持久Closed证明并发布。此前任何不确定结果保持Unknown和active记录，不自动重启热点、不猜测删除其他checkpoint或防火墙表。
- 服务重启只重发Unknown，序号保持单调，防止Core把新Unknown误当作旧状态忽略。若Closed证明已持久保存但active删除/输出回复丢失，可仅补发证明，不再激活；旧完成请求仍不能重放。

## 可信启动配置

这是源码层执行入口，尚未接入镜像或正式服务安装器。用法为：

```text
rustsimhub-setup-ap --serve PRIVATE_CONFIG_DIR CORE_INPUT_DIR ROOT_OUTPUT_DIR ROOT_JOURNAL_DIR
```

只允许Root执行；四个路径是可信服务启动参数。目录不由网页传入，不自动创建。输入/输出权限沿用P4F的Core:Root及Root:Core、2750；Root日志和材料目录均Root:Root、0700。固定文件限制为8192字节、单链接普通文件，严格拒绝软链接、错误UID/GID和权限。

材料目录包含0600的config.json与cert.der。config格式1仅接受realm、core_uid（非0）、device_public_key（32字节数组）、ssid、passphrase、uuid；AP口令必须为已有生成器产生的48位小写十六进制，秘密不进入状态/日志。材料摘要同时绑定配置原字节和证书DER；变更后不能沿用旧租约。证书文件只是Root保存的公开证书副本，不读取Edge私钥。

实际配置还必须有独立AP Edge监听：loopback用于启动前检查，固定AP地址用于连接后的验证，Origin精确为https://172.31.85.1:8443。当前生产Edge与证书安装流程尚未提供这套AP实例，本轮不会为通过检查修改现有服务或信任库。材料/证书生成与可信交付工具仍需完成。

## 验证证据及边界

- 完整工作区301项测试、严格Clippy、生产Core检查通过；执行程序及库ARM64产物核对机器类型183。产物未签名、未嵌入冻结源码包证明，不用于设备部署。
- 执行程序新增12项测试，配网包共26项：唯一激活/持久消费/取消回退、租约过期中断阻塞激活、隔离或回退失败保留Unknown、错误材料/失去HTTPS就绪/调用方停止的无副作用、重启不重放及丢失回执补发、无线Ready但AP HTTPS未Ready、Ready后丢失网页、Unknown序号不回退，以及停止期间等待清理。
- 真实本地TLS握手覆盖正确AP SAN/证书、错误SAN/叶证书、错误设备身份/请求ID/Origin响应和核心错误；材料修改即使在缓存内也立即失效。HTTP解析拒绝错误状态、重复长度、分块/歧义、非JSON、截断完成误判及多余数据。
- NetworkManager与nft分别沿用此前的p2p D-Bus和私有网络命名空间证据；本轮执行状态机的网络/隔离使用夹具，未在真无线或一套真实NM+nft+Edge上做联合实测。
- 网页源码未修改；原12组网页证据保留。精确新提交的GitHub CI结果另存，不把旧提交成功当作本轮通过。

私有证据：setup-ap-executor-20261024-fourth.log/.exit、setup-ap-executor-arm-20261024-first.json及setup-ap-executor-checkpoint-20261024-first.json。最初定向编译因夹具JSON宏中的数组重复表达式失败，已修正并保留失败日志；第三轮300项通过后又补真实丢失回执路径，最终301项通过。

## 仍需完成

1. 固定版本材料/证书准备和可信交付、AP Edge安装/监听配置及正式受限服务单元；当前没有自动启用脚本或镜像服务。
2. 未认领设备的可信本地首次窗口生产者。当前网络控制API仍只接受已有Owner；不删除或重置setup.used。
3. AP到STA试连接、确认、失败恢复AP及隔离规则交接，随后接页面。现有USB前置条件尚未被移除，不能把当前事务末尾的回退当作保存STA。
4. Root崩溃后遗留Unknown资源的明确核对/恢复工具。当前保留隔离与日志并拒绝新Start，既不自动清锁也不推断可安全恢复。
5. 人工方便时建立备用维护入口并实测热点、手机首配、错密码/超时、断电恢复、保存后冷启动。当前维护仍经保存Wi-Fi，用户休息期间不切换无线模式。

当前无需用户操作。PMIC/关机调查继续搁置，普通数据关闭，累计10010仍44。

源码：crates/setup-ap/src/main.rs、executor.rs、executor/material.rs及其tests.rs；持久回执在channel/files.rs。
