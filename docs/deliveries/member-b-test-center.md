# 成员 B 第三批：独立测试中心与浏览器夹具

账号 `wnfhsj88`（旧账号 `wnfhsj`），稳定 ID `member-b`。用户继续指示下交付 B-09～B-12 / WEB-01、WEB-05 的独立组件与离线检查，提交负责人审核。#74、#75 仍待审核，不将它们或本批标成已验收。

- 起始 main：`7257cfa8d82bac8972158ef79ea081d9be12af75`
- 实现/测试/接线提案完整 HEAD：`bc7b40a13b4981c790d569fe334ccc13943495da`
- 分支：`codex/wnfhsj88-test-center`，仓库 `Viper-Boss/RustSimHub`；最终 PR HEAD 见 PR 正文。
- 文件：独立 `test-center.js`、`tools/test-web-test-center-component.mjs`、`tools/fixtures/test-center/{index.html,browser.py}`、`docs/web/test-center-integration.md` 和本报告。

## 实际交付

B-09：已盘点真实首页和 app.js 的五项测试/停止/原操作恢复、登录与音频可用条件、默认收起的调试入口。主线已经实现实际业务，不把它重写或当成本人新增。完整入口/状态表见接线文档。

B-10：新增显式注入回调的展示组件。默认不接线/无明确 enabled 状态时禁用；渲染和更新不执行操作；按钮原因可见并通过 aria-describedby 关联。五项主要任务、停止和查询原操作复用宿主控制器。未包含登录、短信/电话状态机、网络 API、IndexedDB、原操作 DTO/密钥/会话等实现。

B-11：防止并发双击；错误不自动重试或显示成功，未确认结果挡住新提交，保留查询入口。停止处理中防重；停止失败保持未确认，重复点击不会再调停止回调，需宿主明确更新。回调收到显示作用域，reset 后旧回调及作用域通知不恢复已退出 UI。Promise 返回本身不宣称已挂断/停进程，只有宿主实际控制器通知提供业务状态。

B-12：真实 Chromium 三种尺寸测试及截图输出。长中文提示/禁用原因完整换行无横向溢出；手动调试默认关闭；Tab 跳过禁用按钮，Enter 单次调用 check，Tab 到 recover/summary，Enter 展开调试。夹具仅本地合成，不发业务请求。

## 验证（本 HEAD 实际执行）

- `node tools/test-web-test-center-component.mjs`：退出 0。直接执行真实组件；覆盖默认禁用/无渲染提交、未登录/权限/SIM/配置原因、双击、HTTP409/丢回复、Unknown、停止中/停止失败、退出迟到、文本注入。
- 同脚本绑定真实 CallTest，合成外部音频/IPC：麦克风拒绝、权限不足、SIM 未注册、HTTP409 均不产生 prepare/submit；绑定真实 archiveEndedOperation：只查询原 operation_id，Unknown 不归档/不清记录。测试未复制业务实现自证。
- 首次接入真实 CallTest 时夹具错误地模拟 navigator 而未提供 audio.input，断言失败；按真实接口修正合成音频端口后通过，未修改 CallTest 或放松断言。
- `python3 tools/fixtures/test-center/browser.py --output /workspace/setup-logs/test-center-browser`：退出 0，Chromium 1440/390/320 像素全部通过、无 JS 页面异常；截图留在工作区输出目录，未提交。服务只绑定临时 loopback 端口，精确提供 HTML/组件两条路径，阻止非本源请求，结束后停止服务/浏览器。
- `for test in tools/test-web-*.mjs; do node "$test" || exit; done`：退出 0，20 个脚本全部通过（含既有真实 app、CallTest、短信、转发、会话等回归）。
- `cargo test -p rustsimhub-edge --locked`：退出 0，37 项通过。
- `cargo fmt --all -- --check`、`git diff --check`：退出 0。
- 环境：Debian 13 x86_64、Node 24.19.0、Rust 1.98.0、系统 Chromium、Python playwright。无新增项目依赖/锁文件变更。未运行整仓 Rust、ARM64 或真实设备验收。

## 未装配与后续审核

组件目前只在本地夹具挂载，生产 app.js/index.html/Rust 路由/CI 发布均未修改。接线文档给出最小入口映射和步骤；负责人先审接口，再将现有控制器与原 pending/回执/复制/草稿恢复/退出清理接入，并显式加入静态路由和 CI。不能直接把样例 `enabled=true` 用于生产，必须来自现有权限/操作真实状态。显示作用域不替代业务 viewEpoch/testGeneration/current fence。

此交付不宣称生产首页已经使用新组件；真实声音、短信/来电/转发结果、第二端、停止进程证明仍由主线验收。不直接发短信/拨号/连设备、不读取生产数据，不因离线自测增加项目完成度。

来源：OpenAI Codex 辅助编写/执行验证，复用仓库现有控制器，新增文件沿用 AGPL-3.0-only，无第三方源码复制。工时未估造、未重写旧 Git 作者。集成前可独立回退本实现提交，不涉及 schema、依赖或设备配置；已集成后应由主线同步撤销接线。名册、审核和全局进度均未修改。
