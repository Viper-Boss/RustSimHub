# B-30：配网/远程脱敏诊断与交付指引

本包提供独立 diagnostics.js 和实际模块回归，未改变业务 API、app.js 或生产静态路由。安装由负责人审核后接线。

输入投影字段：origin、device_pin、certificate、session、remote_path、business、operation。operation 仅接受规范 UUID、有限 type/state 和 submit_started；多余字段完全不进入输出。证书 trusted/untrusted、会话 authenticated/expired/denied、业务 available/unavailable、路径 direct/relay/unreachable 分别显示，缺少证据均 unknown，互不推断。完整地址/公钥不进入复制结果，只有地址协议/是否有效、公钥是否存在；登录令牌、SSID、口令、短信/联系人、lease 和任意错误原文不复制。

DiagnosticPanel 的 queryOperation(id,{signal})、copy(text) 由宿主显式注入；没有自动请求、发现、重试、提交、归档或任意控制。只查询当前显示的原 ID，3 秒截止/退出隔离/并发防重；错 ID 的响应保留原记录。即使查询成功，也不生成“已挂断”“已恢复”“已上线”等额外结论。copy 失败给手动选择文本提示，不读取其他页面内容。

## 给使用者的支持步骤

1. 从设备已有管理渠道核对当前设备名称/网址，通常使用已安装文档对应的 HTTPS 地址或设备名 .local:8443；不信任陌生恢复链接或广播名。当前组件不会生成可点击的任意 URL。
2. 对照主线设备安装记录核对证书和完整设备 Ed25519 公钥。短指纹/诊断中的 pin_present 不是配对证据；不能从广告名、MAC、GATT 连接或 UI verified 标志批准身份。不得复制 Owner 私钥到 Pages。
3. 浏览器按设备正式安装文档处理信任，然后通过既有认证入口登录。TLS、业务认证、SIM/电话服务、远程直连/中继分别核对，未知条件保留 unknown。
4. 已提交操作未确认时仅使用原 ID 查询，不重复拨号、发送、重置幂等键或清除 IndexedDB。诊断本身不执行修改。远程不可达时保留原操作和本地维护入口，不能隐式启用普通蜂窝数据或切换目的地。
5. 提交问题时复制脱敏诊断即可；地址、完整公钥和真实通信资料仍按主线私有渠道处理，禁止公开原始硬件日志/数据库/备份。

## 接线与验证

负责人在已认证页面提供最小字段投影，queryOperation 绑定已有 call_operation/sms_operation 或已批准的 Wi-Fi/转发查询适配，不把诊断当新权限。结果须映射回相同 ID。logout/pagehide reset，复制只在用户点击后调用 clipboard；静态 allowlist 路由由主线添加。真实设备业务/远程状态提供者缺失时不填写 available/direct。

`node tools/test-web-diagnostics.mjs` 直接加载实际模块，覆盖未知/恶意 URL、缺省和独立状态、HTML/秘密字段排除、精确 ID 查询、重复/退出/错 ID 回复。未访问设备、NAS、外部网络，未改变原业务模块。生产接线、实际证书/地址/远程/Android 行为仍需主线补验。
