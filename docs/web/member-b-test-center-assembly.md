# B-29：测试中心实际首页装配候选

这是 [member-b-test-center-assembly.patch](member-b-test-center-assembly.patch) 的应用说明。补丁尚未应用到生产 app.js/HTML；它依赖已经合入文档、但尚未实际注册的 [静态路由补丁](member-b-static-routes.patch)。不能只增加首页 import 而不提供模块。

源码基线 main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`。
装配补丁 SHA256：`33941d0b701ecfcf81e73b2a11401d8dec52b1584c198b0b676aaffb16c0ff7c`。
受测候选精确为该main + 原静态路由补丁 + 此装配补丁。

## 行为和范围

TestCenter 只调用原先已经绑定的 check/outgoing/incoming/sms/forwarding/stop/recover onclick，保留 act、AP/电话/租约、认证、原ID、写前标记和业务代次门禁。没有新的短信/电话/转发提交器或登录过程。

原七个处理器节点隐藏并保留，唯一可见任务入口为新组件；原结果复制/草稿恢复/原操作提示与手动电话、短信、设备管理保留。页面明确列出10010、约30秒自动挂断、10010/CXYE确认后一次发送和Bark合成通知，不能只剩缺少目的地的抽象按钮。

状态从原禁用条件及当前AP/电话/租约/原操作投影。MutationObserver只同步原面板的显示变化，不读网络或启动业务；观察器不观察自身渲染区域。pagehide重置显示代次并断开观察，pageshow只恢复显示；logout复用原清理并重置组件，不以显示重置替代挂断/设备回滚。初始化失败的原固定提示同步到可见组件。

候选修改 app.js、index.html、六个已有VM夹具以及一个实际首页浏览器夹具。五个其他业务VM夹具仅stub新增展示依赖，保留原业务断言；test-web-test-center.mjs实际加载App和TestCenter，直接验证新入口委托原处理器，不用展示stub自证装配。

## 主线应用和验证

由负责人在审核分支先应用静态路由，再应用装配；当前生产文件未在此PR改变：

```sh
git apply --check docs/web/member-b-static-routes.patch
git apply docs/web/member-b-static-routes.patch
git apply --check docs/web/member-b-test-center-assembly.patch
git apply docs/web/member-b-test-center-assembly.patch
node tools/test-web-test-center.mjs
python3 tools/fixtures/test-center/assembly-browser.py --output /tmp/test-center-assembly-evidence
cargo test -p rustsimhub-edge --all-features --locked --offline
cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings
```

本次在临时副本执行全部25个网页脚本，随后新增入口的SMS/Bark、页面返回/退出断言在对应脚本再次通过；原断言没有删除。实际App/组件验证检查状态原三条只读命令、SMS草稿/一次提交、Bark原处理器/共享IDB/原请求归档、Unknown精确ID查询、AP禁止原因、停止可达、离页迟到与退出隔离。

真实 Chromium 1440/390/320 使用实际HTML/App/模块和Edge CSP。服务器静态允许表从候选Rust include_str清单提取，API仅本地合成403；初始只有identity，手动检查只追加challenge，没有业务提交。验证单一可见按钮组、44px控件/键盘焦点、初始化失败显示、复制入口、原面板展开无溢出和原生显示观察器；CSP违规为0。

合成等待条件初轮误用字符串轮询触发CSP unsafe-eval，改为函数形式后通过；没有放宽产品CSP。组合候选Edge 38项、严格Clippy、fmt退出0；正向/反向补丁检查通过。实际HTTPS/设备、真实权限/声音/短信/Bark和第二端仍未验收。

## 接续与回退

本候选只装配测试中心，不伪装另外四个组件的统计/来源/服务provider已经就绪。其他回执/配置/诊断/统计的接线仍按各自来源合同评审；完整恢复生产后端和发布路径前置不变。

回退时先撤装配 import/页面/夹具变更，再考虑移除新增静态路由，避免留下404 import；不删除原请求、草稿、回执或身份数据。此处的离线通过不增加全局进度，不等于生产装配或全32步最终验收。
