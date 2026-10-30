# B-21～28 / B-31：注入式恢复配置器与静态材料

本包按主线 `docs/team/interfaces/t5-offline-v1.json` 的 frozen_offline_boundary 实现离线 UI/元数据/静态边界。该清单 production_ready=false、crypto_wire_frozen=false；本包不改变这些事实。#76/#79 展示契约仍待审核，因此不导入它们、不装配生产日常页面；以下仅作为可评审候选。默认无生产 client，页面明确不可用，按钮禁止设备操作。

## B-21～24：端口、状态与持久化

- 宿主调用 app.js 的 mount({client,store})，client 必须是主线认证的 RecoveryClientV1。connect 在点击栈内直接调用 connectFromGesture，再等待存储与状态。没有 query/localStorage/隐藏按钮的 Mock 开关，无广播名/MAC/verified flag 认领，没有复制 Owner 私钥。
- pin_mismatch/not_preapproved/revoked/auth_failed/permission_denied 使用有限错误码；没有后端时明确显示尚未安装。原始错误/帧/任意恢复 URL 不进入 DOM。
- begin/confirm/cancel 均是明确动作；confirmed ready、剩余期限与 trial64hex 校验，超过 40 秒本地剩余期拒绝确认；主线端口还必须在每次写和后台重新验 grant、网络锁、电话空闲和普通数据关闭。UI 不承担这些授权。
- 保存只接受有效 ready；明确取消可接受同一试连当前有效的 starting/connecting/ready 证明。其他未知请求继续阻止新写；取消丢回复后保留原 ID，只查询。已证实保存/回滚的试连不会因归档取消回执而重新启用旧快照；重新连接后必须重新查询原请求。
- 严格字段/类型/版本/state/proof 校验，生产控制器只接受 source=core，拒绝 mock 源和多余字段。saved/rolled_back 要求对应 authoritative/settings/rollback 证明；not_found 不是未执行证明。
- PendingStore 只保存 `{contract,device_pin,revision,requests:[{request_id,action,trial_id,submit_started}]}`。每笔写先单事务 CAS 保存 submit_started，再调用端口；失败/冲突禁止写。每设备最多16笔，重开先加载原 ID，不自动发送；未知状态不能清除或归档，只有有效终态证据后人工 archive。请求 UUID 与 trial64hex 不混用。
- `save(next,expectedRevision)` 明确返回 true 才算 CAS 成功；false/异常均拒绝发送。本元数据库名 rustsimhub-recovery-metadata-v1，不读写主线身份绑定库、CryptoKey 或凭据。主线若提供其他 Store，需要按此安全返回合同适配，不把 void 当成功。
- 一次查询3秒、前台每秒最多1次、每次连接最多30次，无自动轮询。隐藏页拒绝查询，口令/SSID 输入清空；断开仅释放本地会话，不声明已取消/回滚。丢回复、Abort、过期、CAS 更新失败保持可能未知，不自动重试。
- Core调用在进入端口前注册计时器，返回后以单调经过时间再次核验；暂停后迟到、时钟倒退或无效的回复不进入操作证明。见 [读取时限](../web/read-deadlines.md)，UI不能抢占同步端口或代替Core真实期限检查。
- SSID/口令只在本次调用内存中，绝不存 IDB/URL/日志/缓存；清引用不能证明 JS 内存擦除。输出 view 只包含有限状态/请求元数据，无身份密钥或业务正文。

## B-25～26：PWA 与更新

manifest/start/scope 均相对构建，浏览器固定路径 `/RustSimHub/configurator/`。白名单9个静态文件（含 SVG 图标），≤32个、每个≤512KiB、总≤2MiB；构建器校验列表/路径/链接/大小，安装流式限制响应。Service Worker 仅处理同源允许清单的 GET，不处理 POST、查询串、Authorization、设备接口、测试/Mock/身份库。网络取静态文件 credentials=omit，不保存响应凭据。

静态源摘要生成独立 cache 版本，候选安装不会覆盖旧 cache。没有自动 skipWaiting、clients.claim 或刷新。明确更新动作还检查会话/原请求；waiting worker 独立检查元数据及其他配置器窗口，有未确认/未知状态或其他窗口时保留旧 worker/资源。即使允许激活也不自动导航/重提写入。首次离线未缓存不可用；已缓存页面能打开，但不因此宣称设备恢复功能可用。

## B-27：实际验证与保留范围

```sh
node tools/test-ble-configurator.mjs
python3 -m unittest discover -s tools/site-build -p 'test_*.py' -v
python3 configurator/tests/browser.py --output /workspace/setup-logs/configurator-browser
```

Node 直接执行交付控制器/严格合同，外部端口明确为合成 Core 格式回复，不证明真实认证；覆盖 gesture-before-await、写前标记、丢回复/重开只查询、存储冲突、过期确认、断开/旧连接、30次查询、非法凭据/证据、多余字段、Mock 拒绝和冻结 Wi-Fi 向量。帧向量只检查冻结头/长度边界，未实现/验证 D 的实际 GATT 分片或密码握手。

Chromium 1440/390/320 检查默认不可用、Tab/布局/无溢出、已缓存后离线重开；实际 IndexedDB 跨连接并发 CAS 仅一个成功，实际 IDB 丢回复后保留原 ID 查询；实际等待 SW 拒绝未确认记录/其他窗口，并在明确允许后激活而不强制刷新。真实点击中的 userActivation 到达注入选择回调；这不是一次真实蓝牙原生选择器/设备认证测试。截图只含合成数据，不发布。

## B-28 / 主线接手

BLE4-01：注入端口/手势/缺后端/预绑定/公钥错误离线检查；BLE4-02：有界试连和明确确认/取消候选；BLE4-03：真实 CAS/丢回复/刷新查询；BLE4-04：静态 PWA/更新保护和 UI 回归。完整设备操作均未验收。

主线必须提供真实生产后端与非导出恢复身份绑定、授权/撤销、认证加密与 D 的 GATT 适配、Core 请求映射/恢复查询和真实 TTL/清理证据；本包没有这些实现，不允许将合成端口安装到生产。还需审核 #76/#79、Store 返回约定、实际 UI 接线和 Chrome/Android/第二端/设备验证。

## B-31：严格组装与发布提案

```sh
python3 tools/site-build/build.py                      # 仅计划
python3 tools/site-build/build.py --run --output /tmp/new-static-site
```

只组装已存在的3个看板文件/.nojekyll及9个配置器静态文件，不扫目录复制、不带 tests/Mock/私有目录/源码报告；输出必须不存在，临时组装后改名。默认不发布。根看板原文件保持逐字节复制。

待负责人确认路径后，现有 progress-pages.yml 的 `Assemble public allowlisted artifact` 步骤可替换为：

```yaml
- name: Assemble reviewed public artifact
  run: python3 tools/site-build/build.py --run --output _site
```

并把 configurator/**、tools/site-build/** 加入原 workflow 的 paths、把构建器测试加到原验证步骤；继续沿用唯一 upload/deploy flow，禁止新建覆盖根站点的发布流。本 PR 不改 Actions、不执行发布。

**路径前置尚未解决**：用户给出的现有看板是 `/RustSimHub-progress/`，与冻结 `/RustSimHub/configurator/` 不同。在前者直接挂载会使 scope/path 检查拒绝注册，不能宣称已发布可用。负责人需确认托管/统一站点方案或正式修订冻结路径，再做以上 workflow 修改、HTTPS 字节复核与 BLE5-02 验收。
