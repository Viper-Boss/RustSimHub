# 应用66首次配网问题：Codex接手与修复

日期2026-10-29。用户提供两张iOS页面截图并确认：点击开始试连后最多2秒就显示“操作结果未确认”，热点没有断开；附近Wi-Fi列表为空。本文件区分复现、离线通过和设备验收。

## 来源和隔离

- CC交接来源：PR62，固定`4cd8dc446417bac882395fc66a6e01b6551c94ca`；六项未合并实测修复全部保留。
- 本批修复代码：`2b172f9330bf64bfb765cd3627004b61f26818be`，独立`codex/ap66-repair-20261029`。本机另建工作树；原D任务保持暂停，未提交代码未动；Claude工作区未改。
- 用户截图/历史部署报告不是本批已成功保存Wi-Fi的证据。当前只读复核确认epoch66、7个服务active、原恢复定时器inactive，USB维护隧道和固定SSH主机公钥可用。没有开启热点、切测试身份、重启、安装或写分区。

## 已复现的立即失败

首次创建Owner后，真正共享的页面控制器在`prepareParent`中发送`setup_ap_start`，把已经开启的匿名设备窗口交给刚建立的Owner，然后才发送`setup_ap_wifi_begin`。

但[HTTP captive白名单](../crates/edge/src/linux/captive.rs)没有`SetupApStart`。因此前一请求直接404，Core从未收到接管/试连请求，页面已保存操作元数据后进入unknown。这与“2秒、热点不动”的反馈直接吻合，不能据此判断密码错误。

新增真实Axum路由反例：合法Origin/Host/JSON的接管请求本应进入Core转发（测试故意没有Core，故正确边界结果为503），旧代码实际返回404，测试失败。修正只加入这一个有限命令，Core原近期Owner/真实device claim/当前Root轮次绑定检查保持。短信、通话、配对、普通设置码认领等仍不在此HTTP入口开放。

## Wi-Fi列表的三个缺口

1. 原逻辑只要实时缓存非空就不用开热点前快照，即使原始缓存全部是不可选的开放/不支持网络。网页再过滤`selectable`后就只剩手动输入。现在判断是否存在可选WPA2项，无可选项才考虑有效快照，安全类型仍不放宽。
2. Owner接管后匿名bootstrap窗口已经结束，但相同开放热点仍在。原列表查询只看匿名window的ready，接管后立即失去快照。新增**只读catalogue投影**涵盖接管后的首配会话，匿名Owner认领仍使用原严格`first_setup_ready`，没有扩权。
3. 首配轮次现已1小时，显示快照仍10分钟到期。现在只读快照上限跟随既有`MAX_WINDOW_SECONDS`；每轮捕获前清掉上一轮缓存，并返回显示用的`snapshot_age_seconds`。网页明确标为开热点前缓存/年龄和读取时间，不冒称实时扫描。超过本轮上限仍拒绝快照。

没有新增RequestScan、无线模式切换、APN/WWAN修改。若开启热点前本就没有任何可选缓存，仍须手动输入，页面会明确解释；本包不承诺单射频在热点开启期间支持实时扫描。

## 测试与旧测试盲点

原`test-web-setup.mjs`只测试创建/登录/列网络，未点击开始试连；未知命令默认返回一个宽松idle对象。DOM桩`replaceChildren`也没有保留传入的placeholder。现在未知命令报错，DOM桩保留真实placeholder，新增完整共享控制器的接管→试连→显式确认保存，核对一次提交/回执字段、秘密不进入持久元数据；另外覆盖非空却不可选列表和12分钟缓存标签。外部Root/设备仍模拟，不把网页测试当硬件成功。

- 真实路由失败基线归档`1e7616d89303d0b21126f24210651bf7bccf95b39b40fea287863465a75d43e0`，日志`9681915737937c0df2edbc5c30379a369395068cbac24fe5eae771e26e8ebd89`，0通过/1失败（404与503不符）。
- 最终修复冻结归档`683f77032a79c4bab9c37978543ae8fb9c9552504edcc79ff5ca5c693a7b4a8b`，日志`a8b13960f9916052fa47929a1986e4b7160e3f3a15376b3150bab5687fa41657`：Edge/Core/Auth/setup-ap四包354项通过、0忽略，严格Clippy和fmt通过。环境为隔离Linux/Rust1.98，实际UID65534、无网络/设备、空capabilities。
- 全部18组网页测试通过；增加空/老缓存断言后，当前修改的首次设置测试再次通过。
- 缓存追加测试初版354通过但一处手写范围触发Clippy告警；改为Range::contains后重新运行上述最终全部检查，保留原失败日志，不关闭lint。
- 只读现场采集完整保存，但本地汇总第一次因设备JSON无末尾换行产生JSONDecodeError；独立raw_decode正确解析得到66和7服务。失败与纠正摘要都私下保留，没有重复设备写操作。

验证命令：
```text
cargo test -p rustsimhub-edge -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-setup-ap --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-edge -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-setup-ap --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
node tools/test-web-setup.mjs
node tools/test-web-ap-wifi.mjs
```

## 候选与验收边界

已从代码2b172f9导出逐Git blob核对的838文件归档，SHA-256`f4e26bf048044991ed185663276b2b18edea7671a5a05f45b4022ab4ceede919`，本机ARM交叉构建正在进行；此句不是构建通过或已部署声明。候选独立前缀app67-codex，不能复用CC的已用尝试标记，也不能盲目全局替换版本数字（签名长度仍64）。构建/签名/实机证据只存忽略的私有目录。

下一步：完成同源ARM/原生音频、真实ARM更新器正反验签、当前设备只读预检与具体备份/回退方案；正常审核并取得本轮安装/测试切换授权后，才进入设备。复测必须记录实际HTTP接管不再404、试连发生并返回热点、显式保存的Root证明、重连后登录和真实数据恢复。未完成前不计AP-07/08闭环，不增加整体百分比。

## 追加：候选已完成，本轮尚未安装

冻结代码2b172f9的12个ARM程序、33项原生音频QEMU测试通过；同源应用67按设备既有公钥匹配的本地密钥签名，未生成/导入新的应用信任。6个文件、22475585字节；真实ARM更新器在QEMU确认67包，错误外部密钥、篡改payload及旧schema负例拒绝。manifest摘要`92c1b122956a7a3e321e898c039ea7e73e6977b6f05081123492aaffc96f3315`。新只读预检确认当前66、7服务active、普通数据断开、无活动通话、恢复额度未用、安装契约与候选一致。

用户正在确认是否开始本轮安装和隔离手机复测；收到确认前不上传/安装、不停业务、不切测试数据。首次构建准备曾因脚本文件名误写而在启动前失败，修正准确映射后才完成上述构建；源归档和签名大小未改，失败未冒记成功。原D任务继续暂停。
