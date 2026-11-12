# WEB5-CONSOLE · 2026-11-11

本包八步已完成生产改动和隔离自测，待负责人审阅 PR；没有合并、部署或接触设备。整体 **81.5%**、20/169、加权10.6保持原值，四包32步不作为169的新分母。

产品固定提交 `77abd18d786fb329f1a6f4529391d0ec1e168462`；`git archive` SHA-256 `6e4415d101531e45171f3083a6b6c545d6a8f7ba246310a3290ef6b4abb6f183`。开发基线为真实 main `7535535bc5a95c42ff1ea70743b97be561f2049a`，任务书固定 `10960dd76baa08b15e4235c43da3111d7d82c635`，没有合入文档分支或重做任务8。本包不依赖 PR215。

## 已有实现与此次改动

| 步骤 | 已有依据 | 本次完成及验证 |
| --- | --- | --- |
| 1 | PR218 安装文档、应用74追加记录；PR220已有recent接口/密码失败回滚 | 逐项重现收到顺序、日期原样/非法日期、终态被reconciling遮盖、分页缓存残留和全库计数。真实127/128原因仍未核实。 |
| 2 | `inbox_recent`/`calls_recent` 既有seq降序游标 | 保留全库收到顺序，不按首屏短信时间重排；刷新重启分页；分页单飞，刷新/退出/线路/库代次隔离，拒绝非递进游标和旧响应。>16条新到、头删、并发、晚回均有确定负对照。 |
| 3 | 存储保留原始timestamp | 严格解析有效日历、秒和时区；有时区转浏览器本地，无时区保留墙钟并标“时区未知”；空/非法显示未知，详情保留受限原文。没有拿当前时间代替。 |
| 4 | 持久化modem_state真实终态7；丢失后availability变reconciling | 只在recent历史投影中保留真实已记录7为Ended。detail/control仍用原映射及原availability，缺来源保持未知；真实guard拒绝、active4、后端丢失和StaleLine控制拒绝均验证。 |
| 5 | recent为设备本地跨历史线路档案，不含短信回收站 | 同一SQLite读事务返回count/list/cursor及只读visibility_seq。该字段取既有sms_deleted/sms_restored事件MAX(seq)，不是时间/伪计数或鉴权。跨页总数变化或visibility变化清除旧页要求刷新。合成128行含历史线路/二进制/1条回收站，实际得到127可见且跨页无漏重。 |
| 6 | 原密码回滚/浏览器钥匙、近期Owner和Unknown幂等保护 | 不改Auth/Wire/scope/schema/入口。原密码6组、session及daily SMS/phone回归通过；实际HTML中错误密码被拒、钥匙未替换、密码框清空，原IDB钥匙再次登录成功。删除近期Owner拒绝后停止不重试的原console回归保留。 |
| 7 | 原HTML/ESM、静态路由、默认CSP、真实IDB/Auth签名 | 实跑320/390/1280三宽，长中文无横向溢出、收到顺序提示可见、65条分页、空/错误/撤权、迟到、切线路/库代次、终态/未知方向、退出/重登；无CSP违规、pageerror、隐式写操作。 |
| 8 | 原storage及console回归 | 同源Rust默认74/74、all-features140/140，strict clippy通过；新Node12组通过，冻结旧生产11组里10组确定失败、1组原退出保护通过。提供本机单命令验证器；保留实际失败及未运行边界。 |

收到顺序是设备入库seq，短信自带时间是内容字段，两者不相同。UI在手机和桌面通信页均说明；有时区时间由浏览器本地时区决定。本包没有新增全库sms_time排序API。总数是相同过滤范围下的设备档案记录数，不是当前SIM收件量、手机原生短信应用数量或去重后条数；验证码由已读取预览识别，未读完显示下界。缺失/无效总数显示`n+`，不补成看似准确的数。未知visibility的旧服务器仅首屏可用，继续翻页会清旧页并要求刷新；旧客户端忽略新增字段的兼容性有Rust真实反序列化用例。

服务端游标仍是`{db_epoch,seq}`。Core `authenticated.rs`原有`serde_json::to_value(auth.inbox_recent(...))`自然输出新增字段，无需修改。visibility来源是本库原删除/恢复事件，与归档全库过滤一致，并由原db_epoch区分数据库代次；没有新增迁移表、Wire命令或权限。

## 实际验证与可复跑命令

在隔离新工作树、Rust1.98.0、已有离线Cargo缓存、Node24、已有Python Playwright和`/usr/bin/chromium`运行。Rust命令为release、opt-level=z、LTO=false、panic=unwind，jobs最多2；没有升级锁文件或安装依赖。74是默认集，140是all-features集，互有重叠，不能相加称214个独立用例。strict clippy是默认storage lib/tests，未声称全workspace或all-features clippy通过。

```bash
# 已有上述工具和离线缓存的本机；不会安装依赖或访问设备。
# 本会话如需选Rust1.98：. /workspace/work/web4-env.sh
./tools/test-web-console-verify.sh
```

验证器依次运行storage默认/all-features测试、storage lib/tests strict clippy、新旧生产console Node对照、密码/session/daily SMS/phone和实际三宽HTML。浏览器只访问临时loopback有限合成API，创建全新临时profile，使用自有临时签名钥匙/IDB；通过原assembly fixture加载真实静态路由/CSP和生产ESM，不使用页面副本或替代业务实现。外部URL全部中止。合成API不代表真实Core后端集成或真实Owner时效。

已执行的分项命令和结果：

- `cargo test --release -p rustsimhub-storage --lib --locked --offline -- --test-threads=1`：74通过，0失败、0忽略。
- 同上加`--all-features`：140通过，0失败、0忽略。
- `cargo clippy --release -p rustsimhub-storage --lib --tests --locked --offline -- -D warnings`：通过。
- `TZ=UTC node tools/test-web-console-fencing.mjs`：原console回归及新增12组通过。
- 同脚本传入从基线7535535提取的真实旧console.js：11组中10组确定失败，原退出护栏1组通过；脚本要求负对照失败数量达到门槛才返回成功。失败涵盖删恢复同总数、头删、>16新到、翻页期间新到、重复并发、晚续页、缺visibility、线路/代次、较早刷新晚回、非法日历。
- `node tools/test-web-password.mjs`：6/6组；`test-web-session.mjs`、`test-web-daily-phone.mjs`、`test-web-daily-sms.mjs`：通过（含近期会话/结果Unknown不重发等既有护栏）。
- `node tools/test-web-console-browser.mjs`：实际320/390/1280三宽全部通过。`node --check`、`bash -n`和`git diff --check`通过。

原始日志、截图、合成原始输入和导出的测试driver仅留`.local/web5-console/`，未提交。四个Rust源文件SHA-256与实际测试driver绑定；冻结产品提交后再次比较4/4相同并重新执行两个导出driver（74/140通过），轻量Node和三宽浏览器也在冻结产品重新执行。本人私有Cargo target已删除释放空间，没有清理他人缓存。产品提交后仅本交付文档增加，不影响受测产品。

## 实际失败与未运行

首轮storage73通过、1失败：新二进制夹具同时带text/data，被原输入校验拒绝；修正夹具清text后74/74通过，未放宽产品校验。Node原断言与新版明确未知/刷新重启语义不符，逐项更新后回归通过。浏览器新增夹具首轮误用不存在的back ID，后误开配置details触发原fixture拒绝；补密码用例时还遇到隐藏输入和HTTP拒绝提示期望错误，均修夹具后实际三宽重跑通过，原失败日志保留。

未运行：整workspace测试/clippy、Core可执行发行构建、真实设备安装/迁移/健康确认、真实Owner撤权/5分钟时效、真实SIM/来电/短信、实际127/128根因、真实浏览器存储、真实手机/设备用户验收、部署及GitHub付费CI。前述发行/安装等属于其他包或本机实测边界，不冒充本包离线自测已完成。没有访问设备/NAS、真实密钥或真实账户，没有发短信/打电话/外发NKN/TURN。

本机后续只需在获批候选安装后核对：历史跨线路/回收站计数、收到顺序提示与实际时间、真实终态7历史显示及控制拒绝、原浏览器钥匙登录、三宽真实观感；127/128仍需只读证据确认。不要为让数字一致清库/恢复真实回收站或导出唯一钥匙。

实现复用RustSimHub现有生产模块与测试夹具，原作者/AGPL-3.0-only标识保留；新代码为本包AI辅助变更，没有引入第三方业务实现。

PR：[223](https://github.com/Viper-Boss/RustSimHub/pull/223)，非草稿、目标main、未合并。远端Rust run42992574622已回读：test/native-audio均`failure`且steps为空；test注释明确为近期付款失败或spending limit导致job未启动。因此GitHub CI记为**未运行（账单阻断）**，不算产品测试失败或CI通过。
