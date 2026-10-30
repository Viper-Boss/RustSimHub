# 成员B：B-29/B-30实际网页装配与静态资源

账号wnfhsj88（原wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-test-center-assembly，PR #100 到main。
最初main `b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`，已正常合入 `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
实际实现/受测源码HEAD：`8ea3f4519e4dee13e1db08301c239e8366c041a3`；PR当前完整HEAD见正文，后续仅文档或明确说明的主线同步。

## 实际改动与范围

本包已从文档候选落实为源码：按已合入 #93 的精确diff注册五个显式静态GET/include_str资源及对应Router回归，接T6允许的最小app/HTML装配。没有变更Rust鉴权、设备API、Core/wire、业务模块、角色或权限。
TestCenter委托原七个onclick，隐藏保留原节点，只有一套可见任务入口；原结果复制/草稿/回执提示/手动功能保留。回执面板只读查询原call/SMS ID，复用原act/query并复核对象/ID/viewEpoch及实际完成；归档仍在原业务区。未知/不支持的本地state不生成saved证明。
诊断只复制白名单投影；不输出地址、完整pin、凭据、正文或lease，不从本地token/pin推出认证/证书/服务状态。原ID查询在回执区，诊断查询未开放。配置/统计组件均已挂载，但read=null、读取禁止、状态/计数未知，不伪造新鲜度或启动引擎/订阅。
显示观察器不触发业务，离页断开并重置、返回只投影当前状态；退出仍走原清理，晚回调不恢复旧视图。没有新提交队列或独立登录。

## 真实验证

在当前实际源码运行，均退出0：

- 全部25个tools/test-web-*.mjs，含加载真实App和五个实际组件的装配回归；配置器Node合同/防重/SSID/取消回归通过。
- `cargo test -p rustsimhub-edge --all-features --locked --offline`：38通过、0失败/忽略。
- Edge全targets/all-features严格Clippy和workspace fmt检查通过。
- `python3 tools/fixtures/test-center/assembly-browser.py --output /workspace/setup-logs/full-web-assembly-browser`：实际首页Chromium 1440/390/320通过。
- 实际路由与原 #93 补丁反向检查通过，保持精确45行资源表及同一回归；diff检查通过。

App/组件验证原状态读取、SMS一次提交/草稿、Bark原处理器/共享IDB/归档、Unknown精确ID查询、AP/停止、退出/离页/返回隔离；新增回执验证重复/错ID/退出拒绝、未知state降级、档案不清除和脱敏复制。五个其他业务VM夹具仅stub显示依赖，原业务断言保留；实际装配测试不使用显示stub自证。
浏览器使用实际HTML/App和CSP，静态表来自实际Rust清单，API为本地合成403；实际IDB保存合成未确认记录并reload，未登录查询禁止。检查单套可见任务、44px/键盘、初始化失败、原记录/复制/面板、原生观察器、配置/统计禁止读取、无溢出和CSP违规0。剪贴板为外部写端口夹具，不声称OS授权实测；请求仅identity/challenge，没有业务提交。
开发中原候选等待字符串触发unsafe-eval，改函数等待；新AP状态夹具补齐原suspend接口后通过。没有放宽CSP、改业务守卫或删除断言。日志/截图仅setup-logs，不提交生成物。未跑实际HTTPS/真实声音、短信、Bark或双端。

## 主线审核与未验收

源码已在PR分支应用，不再要求负责人手动apply文档补丁。旧.patch保留原候选来源，以本PR实际源码/完整HEAD为准；说明见 [装配文档](../web/member-b-test-center-assembly.md)。回退采用正常源码回退，先撤新增import/装配再移除路由，无迁移或数据清理。
本包未跑全workspace/ARM/实机，不借用旧结果填通过；远程CI单独回读。配置新鲜度/统计授权provider、完整恢复认证/GATT/Core后端、Pages路径及真实声音/短信/Bark/Android/第二端验收仍缺。不把默认不可用、合并或自测计为全32步/产品完成，不改全局台账。

沿用AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex辅助实现/验证，没有工时估造、他人作者或分支改写。无设备/NAS/生产数据、真实通信/流量、签名/部署/发布。
