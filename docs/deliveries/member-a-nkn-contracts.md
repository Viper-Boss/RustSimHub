# A-09～12：固定NKN来源、有限请求候选与依赖事实工具

执行账号wnfhsj88，用户2026-10-30明确接手原fungi1220全部任务。分支codex/wnfhsj88-nkn-contracts；起始main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`；实际实现/受测完整HEAD `07c6466b276813fe6cc87bc15f9ed6382c8f9bd2`，之后仅本报告；最终完整HEAD见PR正文。

## 实际交付

1. A-09：从固定nknguard c32952d7586676fec10f71a30b22969cb24881f3只读核对23文件，记录各Git blob/SHA256/字节数；逐模块区分Go NKN、libp2p DHT、成员证明、VPN、Android、NAS装配和统计。根AGPLv3、本地SDK/NCP Apache-2.0保留来源；未复制上游代码或执行其程序。
2. A-10：给可审最小边界与实际纯Go解析器。仅status/stop、协议候选1、规范UUID和1024B单对象；拒绝未知/重复/类型错误/尾随/超量/种子或URL字段、start/路由/交易等。失败返回零请求与有限错误，不回显输入。没有main/stdio进程/真实SDK或主线IPC；不是运行引擎。
3. A-11：实际公开依赖清单生成器，默认只列计划，显式执行两个固定offline/locked Cargo metadata，不构建/下载。区分直接/间接/系统依赖；保留锁文件摘要、包版本/registry校验和与包声明许可。Go require/indirect/sum/本地replace单列，不把官方sum当修改版认证。系统ALSA/Opus/Go逐包许可/漏洞未知保持缺口，completeness=incomplete、approved=false/退出2。
4. A-12：边界/许可/支持矩阵、可重跑工具、真实解析/拒绝测试与具体接口审核点。A-13～16的实际引擎、Linux/ARM制品/EOF/取消/退出，以及A-21+状态/运行监督等没有在本批实现或冒称通过。

## 真实验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -m unittest discover -s tools/dependencies -p 'test_*.py' -v
python3 tools/dependencies/inventory.py
python3 tools/dependencies/inventory.py --run --upstream /explicit/fixed-public-nknguard --output /tmp/new-inventory.json
cd tools/nkn-engine
GOPROXY=off GOSUMDB=off go test ./... -count=1
GOPROXY=off GOSUMDB=off go vet ./...
git diff --check
```

Python6项、Go3项（包含多输入表）、go vet、diff均退出0；默认计划不执行inventory命令。Go实际编译器1.27.3 Linux amd64，module声明最低1.25.7与固定上游一致；没有声称由1.25.7工具链构建，也未下载SDK/模块。

实际清单生成预期退出2，不当成失败掩盖/全部通过：workspace330锁包、native163锁包，包声明许可均取得；跨workspace有重复，不加总为唯一依赖数。固定上游128个require、2个本地replace，后者未获官方sum认证。系统库、Go逐包许可、通告仍未核验。CLI/report保留这些缺口；实际JSON仅setup-logs/member-a-dependency-inventory-final.json，不提交大生成物。

测试核对真实工具的直接/间接关系、未知许可/metadata、无效/重复校验和、Go本地replace、路径/链接/超限、默认无执行；实际Parse覆盖有限命令、重复/转义重复键、截断/第二文档、非法类型/id/协议、URL/seed/start/路由/交易拒绝，失败无可调度请求。未用固定成功stub冒充引擎或网络测试。

## 审核与剩余

重点：SDK构造本身联网并检查真实自发消息；不能OFF/状态读取时调用。DHT Open另开监听/身份/端口映射，WireGuard/Android TUN也不是最小NKN本地库。主线须先审核固定修改版SDK取舍、有限本地控制面与真实启动的身份/Root/端点策略，再开展实际引擎提取。候选细节见docs/nkn/engine-boundary-proposal.md，来源清单fixed-upstream.json，依赖说明docs/dependencies/offline-inventory.md。

没有新增第三方依赖（Go parser仅标准库）、改Core/wire/Root/路由/开机策略/统计库/签名或全局进度。未跑Rust/workspace/浏览器/ARM/设备，不借其他PR899项结果填本包验证。远程CI按最终完整HEAD回读。

无设备/NAS/真实NKN/RPC/消息/链上交易或私有材料读取，未部署/发布环境/固件。AGPL-3.0-only，明确上游Apache来源不被改写；OpenAI Codex辅助实现/验证，未估造工时或修改原作者历史。
