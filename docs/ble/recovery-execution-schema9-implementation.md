# D23 显式 schema9 存储准入与迁移

2026-10-31（Asia/Shanghai）。本包实现已合并接口#103的存储格式阶段。核对到仓库所有者Viper-Boss对#103的[APPROVED记录](https://github.com/Viper-Boss/RustSimHub/pull/103#pullrequestreview-5708198127)并已合并；该记录同时明确没有逐行技术审核或独立测试。以其作为进入实现阶段的流程批准，不把它称为技术/设备验收。新实现另交独立PR及实际测试，不部署生产数据。

## 显式入口

`recovery-execution` feature依赖recovery-grants。默认Store::open/restore仍最高6，即使all-features也不接纳7/9；原open/restore_recovery_grants仍只接精确7。只有下列新入口支持9：

- `Store::migrate_recovery_execution(directory, backup) -> Result<()>`：仅精确7，在同一actor/writer锁内完成Backup API快照，再单个SQLite事务建表、校验、提交9。返回unit后关闭迁移actor，需要调用者另行显式open9；没有先恢复业务状态再备份。
- `Store::open_recovery_execution(directory)`：只接精确9；不初始化缺失库、不从6/7升级、不猜8字段。
- `Store::restore_recovery_execution(backup, destination)`：只从完成备份恢复到新私有目录，保留既有.restore-pending隔离和Owner隔离/realm换代，严格9准入后才交出Store。

6→7仍用原独立显式接口及其备份；9迁移跳过不支持的旧8，6/8/9/未来版本都不能冒充7源。没有原地降级、改号或删表兜底。源未知Recovery对象（含大小写别名）在备份前拒绝，失败不制造“已完成备份”证据。

## 备份和恢复边界

新增私有ExecutionSource打开模式只做原库/格式校验，不执行普通open的recover、line状态修改；因此源7快照在任何业务恢复/格式变化之前完成。备份已存在/失败保持原7和原字节；真实SQLite满盘在DDL阶段失败会事务回滚，保留完整7备份和原数据，没有释放锁重新打开来绕过失败。

普通9重开/恢复将reserved/claimed/starting/connecting/ready/saving改为unknown，保留request/trial/代次/realm、终态及证据，不排队执行或重发；再次打开不把Unknown改成成功/未执行。恢复仍改变现有db_epoch、停用Owner并增加generation；历史grant/执行记录保留，旧能力门禁实际拒绝。坏9恢复目标保留.restore-pending，完成备份源保持不变。

## 精确准入

DDL来自#103已提交的7→9 SQL，执行版只改变注释。严格比较Recovery相关表/索引/trigger/view完整sqlite_schema(SQL、类型、名称、所属表，含自动索引)，区分固定grant模型/执行meta/唯一begin trial索引；不只看user_version。执行行固定format2；meta固定执行模型、identity-hmac-sha256-v2描述和grant_format1。

同时验证现有grant完整结构/FK/容量/管理回执，及执行行的规范UUID、设备pin、Owner/grant代次、固定动作/状态/指纹长度、64小写hex trial、saved/rollback证据。历史代次可保留，但不能高于当前principal/grant；当前相同grant代次必须有相同realm/Owner绑定。confirm/cancel需要同绑定的真实begin行，begin.saved需有同trial的confirm.saved，矛盾终态拒绝。

容量验证为全局最多256行、非cancel最多255，含旧realm和墓碑，不自动删除；合法255+1cancel也实际打开通过。这里是**目录/行准入验证**，不是已经实现reserve/claim或给新操作签发许可。

现有grant管理和Auth读取通过Store内部已验证的OpenMode进入9；原7接口没有自动接纳9。原6→7返回Store的工厂在成功后明确标记Recovery模式，其他调用路径/权限语义保持。

## 实际失败样例

19项新增真实Store/SQLite/Backup API测试覆盖公开迁移/版本拒绝、备份前零恢复写入、备份失败字节保持、SQLiteFull回滚及完成备份恢复、writer锁、旧8混合目录、DDL/meta/索引/trigger/view、规范ID/pin/代次、跨grant/trial、终态证据、Unknown重开/恢复、容量正反边界及恢复隔离。

初稿使用区分大小写的GLOB过滤Recovery对象，新增`Recovery_receipts`大小写变体后，公开迁移的拒绝断言实际失败。已按SQLite标识符语义使用lower(name/tbl_name)后再匹配，原断言和额外view变体通过；没有放宽DDL或删除失败用例。这是实现阶段发现并修正的缺口，不宣称此前主线产品已有同样功能或漏洞。

实际独立main分支整仓 **865通过、0失败、6项继承忽略**；整仓严格Clippy/fmt/diff通过。无feature默认Auth/Storage87通过；只启旧recovery-grants的Storage62通过。19项新增专项及全featureStorage81通过。未新增ignore；proc-macro-error2既有future-incompat提示保留。本分支不含待审BLE组件栈，不把其967结果或本分支数目相加。精确命令/受测HEAD/摘要见[报告](../deliveries/member-d-recovery-execution-schema9.md)。

## 本阶段未实现

执行表现在可显式迁移/打开/恢复和验证，但没有公开请求写入、reserve/一次claim、私有HMAC指纹派生、业务许可、Lookup DTO或Wi-Fi派发；meta中的fingerprint_scheme是已定格式描述，不是已完成密码实现。测试原始插行只属于cfg(test)合成夹具，没有网络写入或假Principal。

后续仍要在真实Auth私有能力/单调fence与同一SQLite writer核验下实现指纹、一次领取及Unknown查询，再接原唯一Wi-Fi actor/网络锁/电话协调/40秒/checkpoint，不另造事务或用Owner替代Recovery。A标准会话、Root/发行与回退9兼容、实际设备/NM/浏览器验收分别保留。

默认生产schema6/既有7实验接口及原作用域保持；没有启动Core服务或部署/签名/读取生产库/真实Wi-Fi/蜂窝/短信/拨号。新格式只有显式库API，不修改Root admission或把当前发行/回退组宣称支持9。

完整步骤新增0，仍21/26离线包已提交、剩D22–26五步，固定P0–P7约80%。不能把表迁移/测试或流程批准标成D23完整验收。
