# 成员B：配置器静态更新的会话竞态保护

账号wnfhsj88（原wnfhsj，member-b），B-26/B-31。起始main `aac1ec246bba9d77c98d42350866d6eb06a414d6`；分支codex/wnfhsj88-static-update-guards；实际实现/受测HEAD `24579bec767af4d79f78a74a8a7644fd72088759`，之后仅本人交付文档，最终PR完整HEAD见正文。

## 可复现问题与修复

原update处理器只在异步读取PendingStore之前检查connected/busy。等待IDB或ServiceWorker注册时开始连接，甚至连接并断开后，旧点击仍发送activate_static_update。新增真实mount/RecoveryController/PendingStore测试在修改前断言失败：连接进行中仍发送1次，预期0。

现在两次读取后均重新核对同一controller.epoch、前台、connected/busy和单调3秒总预算。连接/断开已完成也不能复活旧更新意图。检查期间禁用并守卫新UI连接和重复更新；超时释放本地检查，晚到结果不发送消息。明确发送更新后本页暂停新连接，保持原页面/查询记录，要求用户手动重新打开；不自动刷新、导航或提交设备写入。缺waiting worker是明确无更新，存储未知/出错保持原资源。

只改本人的configurator app与测试、Node入口和集成文档；不改恢复协议、密码/生产backend、身份绑定、Core/Root、静态路径、worker授权、Actions或发布流程。

## 实际验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
node tools/test-ble-configurator.mjs
python3 configurator/tests/browser.py --output /workspace/setup-logs/static-update-browser
python3 -m unittest discover -s tools/site-build -p 'test_*.py' -v
cargo fmt --all -- --check
git diff --check
```

全部退出0。Node新回归加载真实生产mount/controller/store，只替换DOM/IDB/SW外部端口：两处await的连接竞态、连接并断开、后台、重复点击、检查期间/消息发送后UI连接、损坏/不可读元数据、无更新和真实3秒超时后的晚到完成均拒绝不当激活；原CAS/丢回复/只查询/取消/时限等用例继续通过。

真实Chromium 1440/390/320默认拒绝/键盘/布局/离线重开通过；实际IDB CAS/丢回复和waiting worker原pending/其他窗口门禁通过。新增浏览器场景只延迟外部getRegistration，实际mount/controller连接并断开后旧更新被拒绝、真实worker继续waiting；新明确点击才激活，未自动重载，连接/更新按钮保持禁止，原查询记录保留。没有用假worker状态机自证。

站点构建器2项通过；fmt/diff通过。日志member-b-static-update-{red,node,browser}.log与截图仅工作区。未改Rust，未重跑无关workspace/Edge；远程CI按最终HEAD回读，不能借旧结果填通过。

## 未验收与来源

仍未安装生产恢复端口、真实GATT/安全会话/Core事务，未解决冻结/RustSimHub/configurator/与实际看板/RustSimHub-progress/的路径决策；Android/第二端/设备和真实发布由主线安排。本修复不代替生产鉴权、实机恢复或全任务验收。未更新全局进度，未访问设备/NAS、生产数据、真实短信/电话/流量，未签名/部署/发布。

沿用AGPL-3.0-only，无新依赖/第三方代码复制；OpenAI Codex辅助实现/验证，无工时或他人作者改写。回退只撤本次UI门禁，不删除持久请求/身份或旧缓存。
