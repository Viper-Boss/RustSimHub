# A-13～16进程/构建子项：有限stdio前端（真实NKN SDK未接入）

基线main80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d；承接PR115的纯解析器。这里交实际可运行进程、受限帧循环及可复现Linux amd64/arm64制品，**不是完整NKN引擎**。没有链接SDK/统计钱包/Root/网络栈，不能把前端运行、ARM ELF或退出回执当成NKN联网/全局无PID。

## 真实接口与范围

显式`--stdio`只接受其父进程专用管道，默认/未知参数拒绝；`--version`只返回代码SHA/实际Go版本/SDK未链接。不会读取密钥文件、生成身份、开启socket、DNS/自发消息、交易、服务或路由。当前rootless引擎/身份/固定出站/Root lease方案未获审，不通过新增start命令绕过。

帧为4字节大端u32长度+最多1024B请求JSON；最多32帧、单回复<=2048B，复用PR115真实`protocol.Parse`。这是具体候选，替代PR115文中**尚未冻结**的NDJSON建议，未改Core/wire或T5 GATT帧，不声称主线ABI已批准。字段仍只有protocol/id/op，status/stop；额外/重复/版本/URL/种子/start/任意路径均拒绝。错误只有有限类别，失败不采用非法id/回显输入。

status输出完整T5的14字段：source=unavailable、开关/服务/路径/DHT未知、age/字节/RTT/节点数为null、last_error=engine_unavailable。generation=0和有限sequence只是此未接入前端的不可用上下文，不是实际引擎代次/新鲜证据。Result额外标明nkn_sdk_linked=false。未知字段没有编造0/running/connected；未知不等于设备实际关闭。

stop只回复`closing_this_bridge`后结束本前端循环，父进程必须wait核验实际退出；不停止任意主机服务/其他NKN daemon，不宣称Root/cgroup已清空。EOF正常退出，截断/超量帧、读写失败/请求耗尽有限退出。SIGINT/SIGTERM只取消本进程并关闭自己复制/注册的管道端；不发送额外清理网络请求。

继承的os.Stdin/os.Stdout可能未注册Go poller。最初真实二进制探针发现deadline会立即报不支持，已改成只复制本进程专用匿名管道端、设nonblocking并注册owned File；不改其他文件/设备/UID。实际读/写各2秒，头和body共享同一读deadline，不按每字节延长；全进程含version/help有30秒硬退出上限。真实原生测试覆盖2秒读/阻塞写和信号退出，未等待30秒验证硬上限触发；内存测试不替代这些进程检查。stdin/stdout不是专用管道时明确拒绝。

## 构建与来源

```sh
python3 tools/nkn-build/repro_build.py       # 默认计划，不运行Git或编译
python3 tools/nkn-build/repro_build.py --run --output /workspace/artifacts/new-directory
python3 tools/nkn-build/test_native.py --binary /workspace/artifacts/new-directory/nkn-engine-linux-amd64 --output /tmp/new-native-evidence
cd tools/nkn-engine
GOPROXY=off GOSUMDB=off GOTOOLCHAIN=local go test ./... -count=1
GOPROXY=off GOSUMDB=off GOTOOLCHAIN=local go vet ./...
```

实际编译器Go1.27.3 linux/amd64，go.mod最低1.25.7不冒称该版本实际编译。无第三方Go module依赖/下载/vendor或上游示例密钥，只有项目自己的有限前端/解析器及Go标准库；没有把nknguard修改版SDK当作官方包。Go运行时BSD许可随制品保存并记录摘要，项目源码AGPL-3.0-only。

构建脚本只从明确Git HEAD抽取受限公开module文件，未提交构建源拒绝；记录每文件blob/SHA256。每个架构用两份独立源码目录和全新独立GOCACHE，各自完全离线`CGO_ENABLED=0/-trimpath/-buildvcs=false/-buildid=`构建；比较完整二进制、核对ELF64 little endian/e_machine与Go build-info，拒绝泄漏工作区/临时构建路径。输出只到新建隔离目录、不覆盖，不签名/安装/choose/启用服务。

实际受测代码HEAD960d379a00b1f9c34ac044c10288bea62feec029：

| 制品 | 大小 | SHA256 | 实际检查 |
|---|---:|---|---|
| linux-amd64 | 4081530B | a41abe1e314ea05dbcac9f266a2150532350b8ac7d79f29c2bb1817589a25c0d | ELF machine62；两次完整一致；真实原生运行 |
| linux-arm64 | 3980829B | e3f48d7a3855ce7075ccdfaa5886ed2719de51d8cda1e86ab235586d3939520b | ELF machine183；两次完整一致；未运行/未上设备 |

Go3个协议测试+4个bridge测试、go vet通过；原生9类真实子进程检查通过：EOF、不可用状态、stop后wait退出、截断/大小、未知动作/秘密/版本/id/重复字段、32帧、真实部分管道读超时、收到实际status后SIGTERM退出130、实际缩小输出管道不读造成的写超时。没有把早于信号处理注册的默认SIGTERM退出当成处理成功。

真实原生status还交给PR119的实际网页解析器/文本投影，并对照冻结T5 14字段检查：保持不可用/未知/null，不显示0个节点或成功。这证明目前不可用输出形状与候选组件互通，不证明A-22真实SDK字段映射已实现。

完整manifest/Go LICENSE/ELF和原生日志只在工作区隔离制品/证据目录，未把二进制或设备数据提交Git。无ARM模拟器/设备运行、真实NKN/统计/Root/进程低权/cgroup验收；没有正式秘密扫描/许可证批准的整体结论。真实SDK接入、A-22字段采集、A-25 Root监督、密码会话/远程Core桥和生产装配仍待后续实现与接口审核，A-13～16整包不因这些子项标全部完成。
