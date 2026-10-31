# NKN进程与双架构可复现制品：A-13～16子项交付

账号wnfhsj88。main基线80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d；父分支PR115 HEAD e773a57a7e0c0a4d9f5c63850dae22f111f9edb2。实际代码/构建/原生受测HEAD960d379a00b1f9c34ac044c10288bea62feec029；后续仅文档，最终PR HEAD见GitHub，保留旧作者与原接口仍待审核的事实。

交付实际Go stdio前端/有限帧循环、默认不执行的离线构建器、原生子进程验证脚本及docs/nkn/stdio-frontend.md。新增路径仅tools/nkn-engine/{bridge,cmd}、tools/nkn-build及本人的文档。不改Root/Core/wire/环境/开机或生产秘密。

当前程序**未链接真实NKN SDK**，status显式不可用/未知，不返回固定联网成功；stop仅结束此桥接进程，不宣称其他daemon已停。固定请求解析器是真实被执行模块，不复制测试替代。长度前缀是新具体帧候选，未冒称旧NDJSON建议或主线协议已获批准。

实际验证：

- Go1.27.3，module最低1.25.7；完全离线go test（协议3、bridge4）及go vet通过。
- 每架构两份独立源码/独立空GOCACHE构建，总4次；linux/amd64及linux/arm64各自完整二进制相同，ELF架构62/183、Go版本/精确源码信息/无构建绝对路径核对。详细大小与SHA256在说明/manifest。
- 真实amd64子进程9类验证通过，包括实际2秒部分读取/阻塞输出deadline、完整status后SIGTERM处理退出130、stop/EOF/有限帧/秘密不回显。ARM64仅交叉编译，未运行，不等于设备验收。
- 原生实际status通过PR119真实网页模块及冻结T5 14字段投影，未知保持未知；使用的是另一候选PR的真实模块，不冒称main已经装配。
- diff/gofmt检查通过。没有Rust变更，不借用其他分支856/109项测试结果。

日志setup-logs/member-a-nkn-process-{go,build,native-final,status-interop}.log；制品/workspace/artifacts/nkn-stdio-pollable-candidate（不提交）。最初实际探针暴露标准stdio未注册poller，改为仅复制自己的nonblocking pipe后重新完成全部构建及原生验证，未把最初可编译当作可运行。

真实SDK/许可选择、独立身份/固定出站/Root lease、NKN启动/健康/统计交易、实际Core读取与A-22映射、A-25监督、密码/远程服务及最终交接仍未完成；本包没有生产联网/Root/签名/安装，不把A-13～16整包标已验收。主线先审具体接口和SDK来源，再接实际后端。

AGPL-3.0-only，无第三方Go依赖/代码复制，Go标准运行时BSD许可随本地制品保留。OpenAI Codex辅助实现/调试/验证；整体百分比、全局台账保持主线口径。
