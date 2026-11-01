# A13/A16：实际WebSocket-only SDK裁剪候选

账号wnfhsj88，2026-11-01。最初从main
`5d655331f61af4714eff2337a2a19d4f2a6c438e` 新建个人分支
`codex/wnfhsj88-sdk-websocket-only-20261101`，未提交代码前fast-forward到最新main
`c64e52dc414ff02000a7628e118099b01fcbf7d7`，保留已合并PR160及全部主线。
实现/完整受测源码 **`e182f0164b4974b560ae50a3bedd5fe5962b9951`**，首尾Git/input一致且
candidate_sources_dirty=false；之后只补本人报告/来源摘要/索引。最终完整PR HEAD
见PR正文。PR144/153/157/160已由负责人合并，不重复交付，不改其实现或作者。

## 具体实现与实际结果

PR157的WebRTC=false仍编译NKN api/webrtc及Pion。新125行候选diff仅改实际SDK
client.go/config.go：删除peer字段、实现import及两个WebRTC连接分支；配置合并
对显式和继承默认WebRTC=true都先明确拒绝，不分配客户端/重连worker、不拨号、
不静默回落WebSocket。普通WebSocket保留原lifecycle/DialContext/回调/期限。

这不是只打印支持矩阵：工具实际从固定31文件建立原PR157兼容候选阶段，验证
旧补丁，然后正常应用第二补丁并运行真正SDK。补丁基于旧client.go的after摘要；
两层patch都验证digest/目标/前后摘要、fuzz=0，所有其他源和原许可逐字保持。
SDK配置字段/STUN默认字符串及原历史go.mod/go.sum保持来源兼容，不宣称从整个
模块图删除了依赖；实际生成work阶段是独立摘要的派生候选，非修改registry cache。

| 实际检查 | 结果 |
| --- | --- |
| PR157兼容阶段原五项真实SDK场景，go test -race | 5通过，作为本次同条件对照，不算新增 |
| 裁剪阶段同五项 + 两个新WebRTC选择测试，go test -race | 7通过/0失败；新两项各覆盖single/multiclient |
| 原作者SDK两项/NCP两项路由回归，go test -race | 4通过，保留原作者，不算本人新增 |
| Python补丁身份/前层chain篡改/额外目标篡改 | 3通过；篡改即使重算patch摘要也在工具执行前拒绝 |
| go mod verify / Go vet / 修改源gofmt | 退出0，gofmt无输出 |
| native同CGO=0选择比较 | 编译模块闭包37 → 16，移除21；无新增模块/升级 |
| native/ARM64实际go list包闭包 | 304/302包；均强断言无Pion与NKN api/webrtc |
| native/ARM64实际test probe构建 | ELF machine62/183；native制品真实再跑7项通过，ARM未运行 |
| 默认无参数plan | execute=false、network=false；不创建stage/开监听/构建 |
| git diff --check / Python编译 | 退出0 |

比较前后均用Linux amd64、CGO_ENABLED=0包选择；race测试用CGO=1。21个退出编译
闭包的模块包括Pion、DTLS、STUN、uuid、x/net、x/time，完整名单及选中模块的
sum/许可摘要在 `docs/dependencies/nkn-sdk-websocket-only-candidate.json`。
不是完整历史图安全认证，也不是所有被删除模块均有漏洞的断言。

OSV实际查16个模块版本，仍匹配websocket v1.4.2、x/crypto v0.21.0、x/sys v0.18.0
三项；重复GO/GHSA别名不算不同漏洞，没有函数级可达性结论、没有盲目批量升级。
固定来源/补丁与1MiB RPC兼容选择依然待具体审批，不能把已合并157当生产安全批准。

## 精确复现与制品

Go1.27.3，原activate-rustsimhub工具链；GOMODCACHE/GOCACHE/GOPATH明确位于
/workspace/toolchains下。公开固定源码复用已验31文件cache，公开依赖已准备。
本轮执行时GOPROXY=off/GOSUMDB=off、GOWORK=off/GOENV=off/GOTOOLCHAIN=local、
mod=readonly，不静默下载或更新。可选OSV只发送公开模块名和版本。

```sh
python3 tools/nkn-build/sdk-minimal-evaluation/evaluate.py
python3 tools/nkn-build/sdk-minimal-evaluation/evaluate.py --run \
  --source-cache "$PWD/work/a13-eval/upstream" \
  --output "$PWD/work/a13-minimal/run-final" --online-advisories
```

需先设置README列出的非home cache变量；重跑必须换NEW output。完整命令/安全
边界见工具README。前两次探索run允许源码未提交；最终run只绑定以上干净SHA，
不将试跑次数或相同测试重复累加。第一次缺work父目录在任何测试前明确失败，
显式建立父目录后运行，不把该失败记通过。

| 实际测试probe | 字节 | SHA256 |
| --- | --- | --- |
| amd64 | 19249610 | `936d2bb236ee447ef85a92279bbad57a12e2ce03998481467abc300f5d93f07a` |
| arm64 | 17980415 | `5568be81454d37c35adc773af4d6143cad8d367cd7fde641cbf6f94f359c6608` |

这些是测试probe而非stdio引擎、独立空缓存可复现发行包、签名产物或安装；
-buildvcs=false避免生成stage错误继承父Git。ARM编译不算设备运行。测试全部为
合成独立账号/本进程loopback HTTP+WebSocket，其他拨号在DNS/socket前拒绝；
不运行实际WebRTC/STUN。父进程对每个子进程有界，超时只终止自己的进程组。

工作日志 `work/a13-minimal/run-final/` 保留，公开摘要如下：

| 工作日志 | SHA256 |
| --- | --- |
| report.json | `2bc50c536a55a5215a7813b2c8dc62828b7d352a9df5c1e61ebc3be785fe2dcf` |
| candidate-tests.log | `c373a005b68570f80e22ba0a6f75a7e3e1b58b90675b31d56480be99b8bf3369` |
| compatibility-tests.log | `0372e13b5302e0d64719afa27d6e7ebea5d2d5b3d6c88a2d335371a89e036fa3` |
| minimal-tests.log | `8a8a64bfb3f335cc965927f718edcc12b2ff023465b31497629addf9fa762d81` |
| upstream-tests.log | `b24f3e53aa63ce252e6cbc70676ed81c3c28cf7e76b5062fec31ac28350260b8` |
| native-artifact-tests.log | `b7777c2880615b66c3a1d9f05bd11c5bba214dce97ff19e4f69c7611cc449b4d` |
| minimal-patch-apply.log | `d6c0a4c05432a85b3e42fcf9439a378f5ef6fb0daa1d474587124873d2943bd8` |

## 未完成/主线接入

本包仅是经验证的最小SDK派生候选。生产tools/nkn-engine仍无SDK且明确unavailable；
本包不接stdio或新Root/IPC/Auth/密码格式。A13～16仍需两层补丁/依赖处置审批、
真正受管backend与完整进程/敏感输入/两架构独立构建。A22可信状态及A25独立身份/
固定出口/Root/cgroup真实提供者、A26普通Core桥/安全会话、A27应用绑定/crypto
协议、A29生产兼容/Fence/queue、A30完整组合仍缺。9步不标全部完成。

裁剪不证明所有SDK日志已脱敏、所有cache janitor退出或所有密钥被擦除；不守
context的dialer仍需完整受管进程/cgroup边界。公开候选审批后才能计划接线。
没有改根Cargo/lock、crates、configurator、firmware、Actions、默认服务或全局
台账；无真实NKN/RPC/链/交易/邮件/电话/短信/蜂窝/生产设备/NAS/Root/签名/
部署，不自行合并。回退只撤第二层候选文件，不影响现有默认产品和已合并原候选。

新工具/测试AGPL-3.0-only，原SDK/NCP Apache通知、NKNGuard修改与作者保留；
不提交第三方源码/二进制/缓存。OpenAI Codex辅助实现和验证，无工时估造。
主线已验收81.5%和20/169保持。远端CI绑定最终HEAD另行回读，不借其他PR绿灯。
