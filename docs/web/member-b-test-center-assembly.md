# B-29/B-30：首页装配与只读状态

PR #100 功能分支已实际提供五条静态模块路由并装配首页，不再只交提案。路由沿用 #93 已合入的精确GET/include_str diff；Host、中间件、CSP、缓存和所有设备API语义不变。合并/部署与真实设备验收仍由负责人完成。

旧 [装配补丁](member-b-test-center-assembly.patch) 保留最初测试中心候选的审查来源；当前分支不应再次apply。最终实现还包括回执/诊断/配置/统计面板，以当前PR源码为准。

| 面板 | 当前接线 | 保留边界 |
|---|---|---|
| TestCenter | 委托原七个onclick，保留act/认证/AP/电话/租约/代次/原ID防重 | 不新增提交器、自动重试或登录流程；原处理节点隐藏，仅一套可见任务入口 |
| OperationReceipt | 本机原call/SMS记录的有限投影；query复用act/query并绑定同一对象、ID和viewEpoch | 未执行/失败不冒称读成功；只读不清记录，新面板不开放归档，原业务归档仍保留 |
| DiagnosticPanel | 复制有限origin/pin存在性/原操作投影，完整地址/pin/凭据/正文不输出 | 不从token/pin推断认证、证书、业务或远程可用；诊断查询关闭，原ID查询在回执区 |
| ConfigurationState | 已挂载，read=null，读取禁止且状态未知 | 没有新鲜度provider，不把刚收到HTTP或保存意图当进程运行 |
| UsageStatsPanel | 已挂载，read=null，窗口/未知语义保留，读取禁止 | 等A授权读取provider；不编计数、启引擎/续订/订阅或把节点数当人数 |

构造与渲染不请求业务，首页原初始化/认证读取保留。回执拒绝不支持的类型/UUID，未知状态降为unknown，不从本地任意state生成saved证明。支持区默认收起；用户点击原ID查询时仍经过现有业务处理器，局部完成标记仅用于判断原query是否真正完成，不创建第二份提交队列。

原结果复制、草稿恢复、原操作提示和手动电话/短信/设备管理保留。页面明确列出原10010/约30秒、10010/CXYE确认发送和Bark合成通知范围。AP/电话MutationObserver仅同步显示，不读网络；不观察自身渲染区域。离页断开观察并重置组件，返回只投影当前记录，退出沿用原清理。显示重置不是挂断/设备回滚证明。

## 当前源码验证

无需apply补丁，直接在该分支运行：

```sh
node tools/test-web-test-center.mjs
# 全部 tools/test-web-*.mjs 均需执行
node tools/test-ble-configurator.mjs
python3 tools/fixtures/test-center/assembly-browser.py --output /tmp/homepage-evidence
cargo test -p rustsimhub-edge --all-features --locked --offline
cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
```

当前实际工作区25个网页脚本、配置器Node、Edge38项、严格Clippy和fmt通过。实际App与五个显示模块验证SMS一次提交/草稿、Bark原处理器/共享IDB/归档、Unknown精确ID查询、AP阻止、停止、离页/返回/退出迟到隔离，回执重复查询、错ID、未知state降级和脱敏复制。五个其他业务VM夹具只stub新增显示依赖，原业务断言保留；装配脚本加载五个真实模块，不以显示stub自证。

真实Chromium 1440/390/320使用实际首页/模块与原Edge CSP。静态表从实际Rust清单提取；API只做本地合成403；夹具在实际IDB写入合成未确认记录并明确reload，未登录查询禁止。剪贴板写入为外部测试端口，核对实际诊断模块产生的文本不含地址/pin/正文/令牌，未冒充OS剪贴板权限实测。配置/统计读取禁止，原生观察器、44px/键盘、复制/手动面板、无溢出和CSP零违规通过。请求仅原identity/challenge，没有业务提交。

## 来源与接续

历史候选基线main `b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`，原候选补丁SHA256 `33941d0b701ecfcf81e73b2a11401d8dec52b1584c198b0b676aaffb16c0ff7c`，只用于复现原TestCenter阶段；不能拿它代替当前完整diff。静态路由按原精确diff实现，没有扩展到Rust鉴权/设备接口。

此包没有批准配置/统计provider、完整恢复认证/GATT/Core后端或Pages路径。实际HTTPS/设备、真实声音/短信/Bark、Android/第二端仍未验收。回退采用对应源码提交正常回退，先撤新增import/装配再移除路由；不删原请求、草稿、回执或身份。全32步与整体进度按负责人验收记录，不由本地测试或合并自行计完成。
