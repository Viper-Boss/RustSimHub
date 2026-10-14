# 短信观察源的重启恢复

开发修复待实机验收，不增加进度。生产首启测试发现正常核心重启后，9 条原有短信再次作为 9 条候选副本写入。

原观察 backend 含核心鉴权 instance 和本次连接 generation，因此正常重启会改变持久 line_epoch；来源 incarnation 又在新进程重新分配。现在持久线路身份使用系统 D-Bus broker UUID、MM unique owner、SIM/Modem 路径；SIM 私有指纹继续由设备密钥派生。持久 backend 不包含本次进程/连接/信号计数；实际 fence 事件仍先使线路失效。核心鉴权 instance、lease boot、live watcher guard/fence 不变，不能使用持久身份替代它们。

同一已验证线路 ready 后，从自己的 SQLite 恢复当前实际对象列表中已保存的 source/content_hash 和编号上界，再读短信。复用来源保持原消息 ID，不写重复事件；已看到 Added/Deleted/InterfacesRemoved 或来源重置则不恢复旧对象身份，编号仍从既有上界继续。真实新来源即使内容相同也保留，不自动按文本去重。

跨 broker/MM/SIM 或失联仍使线路失效；不可确认的重置、SIM 切换及中断提交仍保持旧失败策略。无需数据库 schema 升级，现有备份/恢复和应用 manifest 的 schema=5 不变。来源元数据不向公网/网页序列化。

需要继续验证：同一实际 MM 下 9 条短信重启前后仍为 9 条且 ID 相同；模拟 Source 退休后的相同内容新消息保留、失联/SIM fence/撤销失败路径不退化。不得据首次初始化/登录成功宣称重复候选已修复。
