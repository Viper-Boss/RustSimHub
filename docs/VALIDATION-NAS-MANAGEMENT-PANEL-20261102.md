# NAS网页管理与条件写入的主线验证

## 本轮交付

在现有网页中装配默认折叠的NAS只读管理：读取已批准NAS/当前绑定、明确保存/撤销、集中查看参数、保存结果不明后的只读核对和本机留存。首页电话/短信/Bark测试入口保持，未用NAS管理替代完整NAS业务。

新增 `bind_legacy_checked` / `revoke_legacy_checked`。近期Owner、当前active NAS、授权代次和只读权限保持；在同一Auth/Authority锁内比较原epoch或明确缺失，然后写入。旧版本或错误身份/代次/权限拒绝，失败不改变账本或有效会话。原管理命令保留兼容，未放宽鉴权。

浏览器先CAS持久化意图，拒绝跨页覆盖；结果不明不会自动重提。意图绑定已验证登录transcript的设备pin，退出/新登录/隐藏后晚回复失效；跨设备记录保持。回执区分正常回复与只观察当前状态，不把后者当原写操作的历史成功。

范围和操作说明：[NAS接入契约](nas-online-bridge.md)、[网页管理装配](web/nas-bridge-integration.md)。无新依赖、设备schema、密钥/密码协议或后台服务；没有变更成员的SDK、安全会话或Root实现。

## 固定来源与本机结果

产品提交 `be007a55b609c276ef84b195f12159a7f9ecd414`；完整源码归档 SHA-256 `f3a7a4a8046154c351c71191d949bd91b71e8de0f8584f1d428dd28f631485d3`。最终文档提交另行记录，不把文档HEAD冒充这些制品来源。

- 同一归档分别解包到独立Windows Node目录与WSL普通用户目录；校验完整归档、拒绝链接/越界路径，复用已核验构建缓存，不称空缓存构建。
- `cargo +1.98.0 fmt --all -- --check` 通过。
- `cargo +1.98.0 clippy --workspace --all-targets --all-features --locked -j 8 -- -D warnings` 通过；原proc-macro-error2未来兼容提示仍保留。
- 仅loopback网络命名空间、降权普通用户执行 `cargo +1.98.0 test --workspace --all-features --locked -j 8`：**1356通过、0失败、6项原有专用忽略**；未新增忽略或联网回退。
- 固定归档中全部 **27组 `tools/test-web*.mjs`** 通过；新增NAS用例执行真实生产模块。六个既有装配夹具补充导入真实NAS模块，保留原断言；初次未注入导致的ReferenceError失败记录保留。
- 真实UnixStream/Core/SQLite覆盖：原始不存在条件首次绑定、重复不存在条件拒绝、正确epoch重绑、旧epoch撤销拒绝、失败后账本字节/既有有效NAS会话保持、授权撤销清理；错误NAS身份、错误代次和写scope均不创建映射。
- JS用例覆盖：严格字段/掩码溢出、确认取消、持久化在发送前、跨页CAS、回复丢失/错回复/归档失败、跨pin、退出/迟到、只读恢复和显式当前状态留存。
- Core/Edge生产ARM64 release构建通过，ELF64/AArch64和独立复制/回读一致。仅编译制品，不是签名发行包，没有在设备执行。

| ARM程序 | 字节数 | SHA-256 |
|---|---:|---|
| rustsimhub-core | 8,002,952 | `31410c1877bffb413c0438d54c5397761275020209e6a0b7631d88255ccf9324` |
| rustsimhub-edge | 3,713,056 | `ab142b99c5b1190ce7a876c0824a38cb44c40f4fda36d5d754fb54cf6a76a9bb` |

权威私有记录：`nas-panel-frozen-20261102-first.json`、`nas-panel-web-frozen-20261102-first.json` 均passed=true；原始日志/源码归档/ARM程序留私有目录。

## 浏览器开发检查与未验收

Chrome扩展实际操作本机loopback合成页，使用真实模块/HTML/CSS，走通读取、保存、丢失回复→只读核对→明确留存，以及撤销。1440/390/320宽度的DOM布局均未横向溢出，可见按钮至少44px；128字符别名和完整参数折叠区可换行。仅为合成开发检查，不涉及真实Owner/NAS或设备授权。

一次全页截图超时后，普通视口截图成功；随后原生截图保存尝试被工具因无法可靠确定浏览器URL而终止。遵循computer-use/guidance.md“stop issuing app input”，本轮没有再发UI输入或改用其他UI技术。视口已在终止前重置，仅本轮loopback服务器已关闭；原生截图文件未保存、用户视觉批准未取得，均不计验收。原始开发记录为私有 `nas-panel-browser-20261102-first.json`。

新Node命令加入既有CI测试步骤，没有删除或放宽门禁。此前云端有付款/消费上限未启动情况，本提交云端结果须独立回读；本机结果不冒称CI通过。

## 后续边界

本轮未签名、部署、操作棒子、修改真实NAS配置或进行真人测试；设备仍按上次应用70记录。需继续真实Owner/NAS配对/配置和授权读取/撤销、正式支持范围及最终用户视觉确认；NAS电话/PCM等成员范围未被本包替代。

原P0–P7验收 **81.5%**、动作 **20/169** 保持。主线代码约90%仅粗估，不因页面或测试数自动加分；目标保持active，人工最后。
