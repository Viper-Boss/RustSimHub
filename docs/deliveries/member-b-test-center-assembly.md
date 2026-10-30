# 成员 B：实际首页装配、只读回执/诊断与转发状态

账号 wnfhsj88（原 wnfhsj，member-b）；仓库 Viper-Boss/RustSimHub；分支 codex/wnfhsj88-test-center-assembly；PR #100，B-19/B-29/B-30。历史起始 main `b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`，已正常合入最新 main `aac1ec246bba9d77c98d42350866d6eb06a414d6`。

实际源码/受测完整 HEAD：`923c3827d3c49707a376c2fabc1ba0d3c3b05778`，含原装配 `8ea3f4519e4dee13e1db08301c239e8366c041a3` 及本批真实转发读取；之后仅本人交付文档。最终 PR 完整 HEAD 见正文，不把旧审核套到新提交。

## 实现与范围

- 按已合入 #93 的精确45行 diff 注册五个静态 GET/include_str 模块及84行 Router 回归。T6 允许的 app/HTML 已装配真实五个组件，不再要求负责人手动 apply 文档补丁。
- TestCenter 委托原七个 onclick，保留 act/认证/AP/电话/租约/代次/防重。只显示一套入口；保留原复制、草稿、恢复提示和手动功能。
- 回执仅投影原 call/SMS 有限字段，原 act/query 核对同一对象、ID、viewEpoch 及实际完成。新面板不开放归档；未知本地 state 不生成 saved 证明。
- 诊断只复制脱敏白名单，不输出完整地址/pin/凭据/正文/lease，不从本地 token/pin 推断认证、证书或业务。原 ID 查询留在回执区。
- 配置面板已接现有 forwarding_status：利用实际 Core 请求开始后的报告核对，严格检查能力/成功/verified/状态/代次；保守计算完整 RTT、毫秒余量与 UI 等待年龄。3秒超时，累计5秒过期；没有有效证明保持未知，不确认保存、不启停/发送。
- 配置读取校验 token/viewEpoch/Abort/可见性/原业务忙碌状态，旧 HTTP 结束前拒绝重叠查询；退出/离页/后台重置，返回不自动请求。保留原 HTTP/Core 时限，不把 UI Abort 冒充设备取消。
- 统计已挂载但 read=null/禁读，等 A 授权 provider；不编计数、启引擎或订阅。其他配置来源不从转发证明外推。

没有新依赖、Core/wire/设备 API/认证/业务提交语义或数据迁移改动。来源论证、接口门禁、原型历史与回退见 [装配说明](../web/member-b-test-center-assembly.md)。

## 本批实际验证

下列命令均退出0；源码即上述923c382完整HEAD：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 逐个运行全部25个 tools/test-web-*.mjs
node tools/test-ble-configurator.mjs
python3 tools/fixtures/test-center/assembly-browser.py --output /workspace/setup-logs/configuration-reader-browser
cargo test -p rustsimhub-core --all-features --locked --offline forwarding::service::tests -- --test-threads=1
cargo test -p rustsimhub-edge --all-features --locked --offline
cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

- 网页25脚本/配置器 Node 通过；配置读取测试在修改前因未接入按钮禁用失败，修改后通过。真实 App 加载五个实际组件；其他五个业务 VM 仅 stub 新展示依赖，原业务断言保留。
- Edge38通过、0失败/忽略；静态源码/MIME、错误 Host403、POST405、未知404及 CSP/permissions/no-store/nosniff 保持。严格 Clippy/fmt 通过。
- Core现有 service 测试7通过、167过滤、0忽略，覆盖真实 dispatch/verified、旧/未来报告、boot/代次/身份不符、Owner/并发拒绝、固定任务失败/超时等。实际使用私有 UnixStream D-Bus，不访问主机服务。未改 Core，未将既有合同测试算作新增后端。
- Chromium 三尺寸1440/390/320：实际首页/CSP、实际 IDB 合成保留记录/403、脱敏复制、单套入口、键盘/44px/溢出/初始化失败和原观察器通过。随后真实 Ed25519 本地合成挑战经过实际 proof.js/浏览器 key 签名和夹具验签，沿原登录流程手动查询转发运行/关闭/未知；真实5秒陈旧、无自动查询、退出禁读，CSP零违规。没有注入 App token/放宽认证。
- 配置 Node 另覆盖完整 RTT/排队年龄、毫秒余量、保存不推断、缺能力/证明、时钟回拨/非有限/3秒边界、退出/离页/后台迟到和旧传输不重叠。原 SMS/Bark/原 ID/草稿/AP/停止与取消断言继续通过。

日志 member-b-configuration-reader-{web,edge,clippy,browser}.log 与 member-b-configuration-core-contract.log、截图均仅工作区；剪贴板为外部写端口夹具，不冒称 OS 权限。浏览器 API 完全本地合成，原登录/状态轮询及 configuration_indicator(false) 保留；新增状态入口没有配置/启停/通知/通信提交。

## 未验收与接续

未运行全 workspace/ARM/实际 HTTPS/设备/真实声音、短信、Bark/Android/第二端；不借旧结果填通过。统计授权读取、完整恢复认证/GATT/Core 后端、Pages 路径及实机/独立技术复核仍待 A/D/负责人。转发运行证明不代替 AP 保存证明或真实设备无 PID 验收。

最终远程 Rust CI 按完整 PR HEAD 回读；此前 run41005330855 因 GitHub 账单/spending limit 未启动、steps为空，不能称 CI 通过。历史旧 HEAD 的流程批准明确未独立审核，不代表本次源码已通过技术审核或功能验收。所有源码仅提交个人 PR，未自行合并、部署、发布或更新全局进度。

沿用 AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex 辅助实现/验证，无工时估造、他人作者/分支改写。无设备/NAS/生产数据、真实通信/流量或签名操作。
