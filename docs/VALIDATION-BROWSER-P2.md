# 实际浏览器、Wi-Fi、会话恢复与只读业务

2026-10-13—14。使用 Codex in-app Chromium 浏览器与隔离设备身份。
设备端服务仍是 VALIDATION-EDGE-P1.md 的只读 core / 不同 UID 的 HTTPS 入口，没有新的短信或拨号。
本阶段补齐浏览器 WebCrypto、IndexedDB key、批准/签名登录、实际短信展示、事件分页与 Wi-Fi 链路验证。

## 连接路径与验收边界

桌面浏览器访问本机回环 QA 入口；测试代理只监听 127.0.0.1，转到已发现的设备 Wi-Fi IPv4 上的 HTTPS。
代理使用 CERT_REQUIRED + check_hostname，只信任通过现有 SSH 维护渠道取得并核对的本次证书，严格限制 Host、Origin、路径与请求大小。
它不持有浏览器私钥、设备 identity 或 owner seed，不自动批准客户端；正式设备的浏览器 HTTPS 信任安装仍待完成。
这是实际浏览器＋桌面至棒子 Wi-Fi 的加密链路，不能描述为浏览器直接通过已安装公共 CA 访问设备。
没有绕过浏览器证书警告或修改 Windows 信任库，没有访问 NAS 业务 API；NAS 只用于维护 SSH 跳板。

## 已通过

1. 浏览器首次载入自动生成独立 Ed25519 key；私钥不可导出，保存在本机 IndexedDB。
2. 通过可信设备 pin 验证 challenge 的签名与绑定；发送只读 client 配对申请。
3. Root 离线 owner 新证明核对实际可见公钥、申请 ID、kind=client、scope mask=11，只批准 status.read / sms.read / calls.read。
4. 原窗口临近过期时请求随后失效；没有复活旧请求，重新打开窗口、新证明与精确新 ID 批准通过。
5. 浏览器签名登录成功，界面实际显示 SIM ready、无活动通话、Wi-Fi/USB 状态及已有短信摘要。
6. 点击已有短信“查看”，summary / sender / text 在 DOM 实际渲染，测试只记录字段和长度，不复制正文到报告。
7. 补同步读取 20 条短信事件、首批 32 条通话事件；下一批 0 / 4，验证实际 cursor 继续读取。
8. 三项只读客户端的合成发送准备被 core 拒绝，确认提交按钮仍 disabled；没有调用 Send。
9. 超时停止后复用同一隔离 state/identity/certificate 启动，旧浏览器 token 被拒绝，新挑战登录成功；公钥与批准关系保持。
10. 退出清空已显示短信和摘要；登录 token 不存在持久浏览器存储。

## 退出期间的读取取消

发现普通 busy 包装会让正在读取时的“退出”点击被忽略，修正为独立的退出处理。
退出先增加 view/action 代次、取消 fetch、清空 token/lease 与敏感输入/显示。
API、签名、配对、登录和异步存储后的界面更新检查代次；旧 continuation 不更新新界面或恢复 busy 状态。
持久提交防重 marker 保留，以便下次登录查询结果不明的操作；未登录时不能提交/查询。

通过 QA 代理仅延迟 line_status 五秒，真实 UI 连续点击刷新/退出：立即显示“未登录”，
延迟响应返回后仍未登录、短信正文/摘要长度均为 0。没有使用 page evaluate 修改内部 JS 状态或伪造登录。
该测试使用当前工作区的 app.js 经回环代理提供；最新静态 release 正在构建，不能把源端 UI 验证当作该 release 已安装。

私有截图 `.private/browser-readonly-login.jpg` 只包含随机测试身份/公钥和登录状态，不含真实短信正文或网络地址，不提交 Git。
原始 UI/操作证据、公开证书、pin 及 fixture 指针保留私有区域。
更换系统、网路写入、网页实际发送/通话写入、麦克风/WebRTC、长期稳定性和冷启动仍待完成。
