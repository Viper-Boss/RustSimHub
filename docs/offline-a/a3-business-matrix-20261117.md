# A3 转发、短信与 NAS：本轮离线证据

对应[A包 A3 七条要求](../team/OFFLINE-A-PRODUCTION-20261115.md)。
起始 main `de663743bd726e77f7c560139d0f91ea78e8fe57`；实现 `72789c7bbaf215ef32f3970394f1d5bf0c62b056`，tree `74a4f2f614d1c1f4218123731458211121f374bb`。
精确命令及原失败见[交付报告](../deliveries/OFFLINE-A-A3-BUSINESS-20261117.md)。

这次交付独立于 A1 草稿268、A2 PR272和B包；从主线开始，不合入其他窗口工作。已有转发修复、SMTP、短信/NAS实现继续复用。
本轮完成可独立运行的补缺和复验；旧NAS页面/旧PCM输入、真实账户/消息/声音、设备及B发行验收仍保留。28个原动作不因此变为已验收，全局仍81.5%。

## A3 七条要求的实际状态

| 要求 | 本轮交付 | 仍缺条件 |
|---|---|---|
| 1 发送器/邮件 | 默认127、SMTP候选129项复验；实际本机TCP/TLS/认证/正文/claim门禁。SMTP原生产接线已存在，未重写 | 真实Webhook/Telegram/邮箱账户和收件证明 |
| 2 Unknown/撤权/错误 | 原转发关闭、凭据换代、迟到响应及重启回归；新增实际SQLite ENOSPC、命名MM MemoryFull、实际NAS丢回执/撤权联测 | 不以合成端点替代真实断网/运营商结果 |
| 3 独立默认OFF | 原queue/supervisor/Core回归，转发默认不运行，释放自己工作；不改电话/NKN/SIM存储生产代码 | 真正PID1/设备冷启动及业务并发 |
| 4 短信视图 | 新增两个真实Auth身份经原IPC分页、回收站权限、SIM来源换代、撤权及重启；原中文/组合Unicode/emoji/长/二进制边界复验 | 手机/双浏览器与真实PDU/运营商分片 |
| 5 发送恢复/写满 | 新增16MiB私有tmpfs中五个授权落盘边界真实ENOSPC；原sender跨重启不重复Create/Send、原ID查询及子进程退出矩阵复验 | ENOSPC新增测试聚焦Storage边界，未在真实modem/设备上写满；不声称新的整链路物理掉电试验 |
| 6 NAS | 修复正常控制客户端缺Edge请求头；实际Go→原Edge TLS→原Unix IPC→原Core Auth/actor联测；只读兼容/导入防重原测试复验 | 固定旧NAS页面源码/版本和完整原UI接入；真实NAS仍未接触 |
| 7 PCM | 原normal-nas-candidate六项Permit/Pump/合成Pcm契约随兼容组合复验 | 没有旧PCM传输规范/时钟映射/真实声音证据；不改native、AirSIM或媒体生产代码 |

## 新缺陷与回归

正常NAS控制客户端 `tools/nas-adapter/control/client.go` 原来只发送JSON Content-Type，缺少原Edge门禁所需的Origin和`X-RustSimHub-Request: 2`。
过去本机模拟HTTP对端不执行原Edge门禁，因此签名/JSON向量通过不能证明该客户端可以进入真正API。
现在只按已验证、固定的`c.origin`加入两个头。原Edge的Host/Origin/JSON/标记门禁、严格CA/主机名、Core权限、原ID与无重试语义保持。

旧代码负对照在实际Edge返回http_denied，Core转发0次。相同Go回归、Core联测夹具和Edge夹具在修复版通过。
完整负向/正向tree另有两处合成测试夹具修正和原Go单元头断言差异；没有声称两个完整tree仅差生产文件。三段NAS联测fixture blob已逐字节核对相同。

正例经原Core认证：同一幂等键得到原操作ID；Submit实际完成后由客户端transport读完并丢弃该回复，再返回合成错误；客户端保留Unknown/RecoveryID，仅查询原操作恢复。
这项故障注入发生在客户端，不能写成物理断网。显式重复原Submit只回读既有状态，真实合成MM Create/Start各1次。
第二普通客户端不能按原调用者的lease执行DTMF，tone0；撤销原调用者后原token被拒绝，hangup1。共9个真正Core转发请求。
合成readonly NAS token不能升格电话控制；控制仍需另行获批的正常Client身份，不转换旧NasKey或Owner钥匙。
运行的是原Edge router/TLS/边界/Unix转发，测试入口不调用完整Edge启动函数，不启动AirSIM/BLE/媒体监听。

## 28 个原动作

表中“复验”限于本轮实际运行的断言。旧报告与失败保留，不能把历史实机描述当当前结果。

| ID | 当前离线证据 | 仍未验收/需输入 |
|---|---|---|
| FWD-06 | 原HttpsSender真实loopback TLS、JSON/认证/原回执ID和队列确认复验 | 真实Webhook投递 |
| FWD-08 | Telegram编码、ok=false/429/Retry-After、单次Unknown复验 | 真实Telegram连接/收件 |
| FWD-09 | 原SMTP生产接线，默认18/候选21个邮件测试；TLS/hostname/AUTH/MIME及DATA前后结果 | 真实邮箱与B正式组合 |
| FWD-13 | 已合并ready回调撤权/截止修复、关闭及迟到结果门禁复验 | 实际PID1停止及设备时序 |
| FWD-14 | 原固定接口/地址/DNS与拒绝回退测试；仅私有lo端点 | 真实USB/Wi-Fi中断恢复；不得蜂窝回退 |
| FWD-16 | 原Owner/realm/credential generation/claim fence/秘密隐藏复验 | 生产凭据轮换与真实通道 |
| FWD-17 | 合成换卡/重复/二进制来源隔离回归 | 实际SIM对象与重复投递现场 |
| FWD-18 | 原默认OFF与无producer/claim回归 | 设备冷启动 |
| FWD-19 | 原有限supervisor和新鲜网络/租约前置复验 | B的服务启动组合、延迟联网与断电 |
| FWD-20 | 原outbox3/worker4和候选outbox4/worker5格式/检查/拒绝回归 | B的完整迁移、回退、签名安装 |
| FWD-21 | 原有界队列/回执/IO/总预算回归；本次未重复历史转发Root进程/ENOSPC专项 | 设备资源、功耗、长稳/实际并发 |
| FWD-22 | 原网页forwarding/history专项、实际主页与控制台三宽 | B发行接线与真实发送流程 |
| SMS-01 | 新增两套真实Client身份经原IPC，以不同页长核对17条合成消息；第二身份继续可读 | 两个真实浏览器/设备同步 |
| SMS-02 | 原墓碑跳页/游标错误/乱序/重开回归；新委派视图跨重启保持ID | 长时间离线与跨设备恢复 |
| SMS-03 | 原MM属性/4096字节/Unicode/二进制边界及新原IPC视图 | 原始运营商PDU/分片实测 |
| SMS-04 | 新MM MobileEquipment.MemoryFull分别在Create/Send返回，真实私有D-Bus/原sender，重复Submit/Query不重复发送；原对象换代回归 | 真SIM满及SIM写入未操作 |
| SMS-06 | 新真实tmpfs ENOSPC覆盖prepare/begin/bind/permit/confirmation；原子进程退出与sender重启回归 | 真机存储故障/物理掉电与完整设备链 |
| SMS-07 | 原真实私有MM对端对账/原ID/撤权/超时回归，MemoryFull保留Unknown | 真实运营商发送/投递证明 |
| SMS-08 | 新回收站Owner权限、委派拒绝/恢复可见、换SIM相同来源新ID、单身份撤权/另一身份保留、重启旧token失效；原备份隔离回归 | 真实多端与备份恢复后的完整网页组合 |
| SMS-09 | 固定SHA、命令、合成日志交B；实际原网页三宽通过 | B签名候选、真实双端短信终验 |
| NAS-01 | 原有限只读API、签名/设备绑定/时间/nonce/版本语义及Core兼容Auth复验 | 对指定旧NAS版本作完整兼容承诺仍需输入 |
| NAS-02 | 原Go只读CLI/严格TLS/有限本机端点与Rust兼容读权限回归 | 原NAS页面源码/准确版本未提供；本机RustSimHub网页不是旧NAS页面 |
| NAS-03 | 新实际Go控制→原Edge→原Core/actor，修请求头；原ID、竞争和撤权通过 | Cancel/Renew本次仍依原Go向量/原Core单元；不声称五方法全部在新增整链路正例执行 |
| NAS-04 | 原normal-nas-candidate Permit、Pump、合成PCM六项契约 | 旧PCM协议、跨主机时钟/传输和真实双向声音 |
| NAS-05 | 原真实SQLite合成旧表格式、预检差异/坏行/未知列/上限/权限回归 | 真实NAS样本未复制；新来源格式需准确schema |
| NAS-06 | 原重复导入/冲突回滚/来源绑定/中断子worker恢复防重复验 | 实际NAS导入/真实恢复组合 |
| NAS-07 | 原兼容session撤权/重启nonce栅栏、旧token失效；新正常控制原ID及实际Auth撤权 | 不将新Go Client进程当真实NAS重启；兼容会话重启沿原单元证据 |
| NAS-08 | 有限只读桥/正常Client控制/PCM候选支持边界及本轮证据交B | 完整旧UI、旧PCM、实际NAS/用户终验 |

## 给 B 的接入和剩余输入

从实现SHA导出，保留Cargo.lock和原依赖；原SMTP使用Core `email-production-candidate`/forwarder `smtp-production-candidate`，本次没有增加依赖、wire、Root或数据库格式。
正常NAS控制仍只提供Prepare/Submit/Operation/Cancel/Renew，新的两个HTTP头不能代替Core认证。
默认Linux/Rust/Go命令及显式ignored场景完整列在交付报告。NAS真实联测必须同时运行本次Core bin test、Edge bin test和Go控制测试；没有fixture环境时Go测试Skip不是通过。
ENOSPC场景必须先建私有mount/net命名空间、独立16MiB tmpfs，检查真实挂载后降权；切勿在主文件系统上制造磁盘满。
原始包中的执行辅助脚本记录了本机目录及缓存前置，属于环境绑定复现材料，不能直接当任意电脑的一键安装器。

仍需负责人提供准确旧NAS页面源码提交/实际版本与读API契约；旧PCM需准确帧格式、采样率、时钟/传输和原媒体权限提供者。
其他外部输入为真实测试账户/目标、用户允许的实际消息测试、设备及B正式发行/回退材料。现阶段不访问这些资源，不发真实消息，不签名/部署，不接管保护声音路径。
热点专用开发/手机验收继续延期，USB优先不变。审核后由负责人处理主线、全局看板和实机验收。
