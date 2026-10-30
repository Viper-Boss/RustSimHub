# 成员B：B-29测试中心首页装配候选

账号wnfhsj88（原wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-test-center-assembly，PR到main。
起始main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`；提案代码提交：`c1ed002d101483459ca9d3ca6cfb0f3eff4b9111`。
正常合入最新main `6b5de5f3ca5ff706adacd8f25ed7f638546739a9`，合并HEAD `32b39c437aae7f36ca0b55532508bfbc89c6e6f8`。新增主线没有Edge/本包网页夹具改动，补丁应用检查继续通过；最终完整PR HEAD见正文。

交付 [可应用补丁](../web/member-b-test-center-assembly.patch) 与 [接线/验证/回退说明](../web/member-b-test-center-assembly.md)。受测源码为起始main + 已交静态路由补丁 + 本补丁SHA256 `33941d0b701ecfcf81e73b2a11401d8dec52b1584c198b0b676aaffb16c0ff7c`，位于临时源码副本。当前仓库生产app/HTML/夹具没有应用，净diff仅本人文档/补丁。

## 实际行为

新组件委托七个原onclick处理器，保留原act/AP/电话/租约/登录/代次及提交防重。原处理节点隐藏而不删除，保持一套可见入口；结果复制、草稿恢复、原操作提示和手动功能不移除。明确展示现有10010/30秒、短信10010/CXYE确认及Bark合成通知范围。
AP/电话显示观察器只更新展示，不发请求；离页断开观察/重置代次，返回只恢复显示，退出仍沿用原清理。初始化失败原固定提示进入新组件，避免隐藏旧提示后一直显示检查中。没有新提交队列、鉴权、业务状态机、租约或恢复provider。

## 真实验证

在组合临时副本执行，以下均退出0：

- 全部25个tools/test-web-*.mjs；最终Bark/SMS新入口及logout/pageshow断言补充后，test-web-test-center.mjs再次通过。
- `python3 tools/fixtures/test-center/assembly-browser.py --output /workspace/setup-logs/test-center-assembly-browser`：真实Chromium 1440/390/320通过。
- `cargo test -p rustsimhub-edge --all-features --locked --offline`：38通过、0失败/忽略。
- 对应Edge全targets/all-features严格Clippy与workspace fmt检查通过。
- 本仓库正向git apply --check与组合副本反向检查通过，补丁与受测源码一致；提交diff检查通过。

App与TestCenter在集成脚本中均为真实模块，验证旧状态三次读取、SMS一次提交/草稿、Bark原处理器/共享IDB/归档、Unknown精确ID只查、AP禁止、停止可达、离页迟到/返回/退出隔离。五个其他业务VM夹具只stub新增展示依赖，原业务断言保留；不是以展示stub证明装配。
浏览器使用实际HTML/App和原CSP，静态表读取候选Rust清单，所有API仅本地合成403。初始只identity、手动检查只challenge；其他面板最多原认证只读请求，无业务提交。单一可见控件组、44px/键盘焦点、初始化失败、复制入口/手动面板、无溢出和原生显示观察器通过，CSP违规0。
初轮浏览器等待字符串轮询触发unsafe-eval，改成函数形式后通过，没有放宽CSP。日志/截图仅setup-logs，不提交生成物。未运行整仓/ARM/设备/HTTPS生产部署/真实声音、短信或Bark/双端验收。

## 主线接手

负责人先在审核分支应用静态路由，再应用本候选并复跑；仅合并本提案文件不会挂载页面。回退先撤装配再考虑移除路由，无数据迁移/清理。
最新D-95仍为运输验证工作项生命周期，没有生产验证器/恢复provider。本候选不把另外四个组件的provider或其他B-29集成伪称就绪；具体缺口沿用各integration文档。自测/合并不等于全32步或实机验收，不改全局进度。

沿用AGPL-3.0-only，无新依赖或第三方源码复制；OpenAI Codex辅助实现/验证，没有工时估造、他人作者重写或他人分支修改。无设备/NAS/生产数据、真实通信/流量、签名/部署/发布；远程CI按PR精确HEAD另行回读。
