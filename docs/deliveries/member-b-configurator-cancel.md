# 成员 B：B-23 已证明试连的明确取消

账号 wnfhsj88，稳定成员 member-b。仓库 Viper-Boss/RustSimHub；分支 codex/wnfhsj88-configurator-cancel；目标 main。
起始 main `b6c73c6a47e00654bff51ae2b221fdcd7e9458e0`；修复提交 `acbcc4326c6029a0a2d95e0960cf8892a9b516b6`。
提交前正常合入最新 main `5238670246e7c6e43184534a7a6ee4b13c293ffd`；合并/受测源码 HEAD `153dc8a4ae6b0183afa6010d9237804e534e1f54`。
PR 当前完整 HEAD 见 PR 正文；本报告及集成说明不改变受测源码。

## 问题和修改

原控制器把所有 starting/connecting 都当成禁止后续写入的未决状态，页面又把取消与 ready-only 保存共用开关。即使已有同一试连当前有效的 Core 格式证明，用户也不能明确取消正在连接的试连。

新增回归先在原实现失败（退出1）；最小修复只为 cancel_wifi 排除同一 begin_wifi 的有效 starting/connecting/ready 证明。保存仍只接受 ready，未知、过期、其他未决操作继续阻止新写。每次取消仍须先完成真实存储 CAS/提交标记，再调用既有注入端口；没有重用请求 ID 或自动重试。

终态试连在当前连接中保留关闭标记；归档取消回执不会让旧 begin 快照重新启用保存或取消。标记随最后引用清除，连接切换清空证明并要求重新查询，不新增持久格式。UI 分别计算确认与取消按钮条件。

改动仅在本人配置器 controller/app、cancel.mjs、浏览器夹具、Node 入口和本人文档。不改 D 的 security/production-backend、A 的 crypto、Core/wire/Auth、发布流程或全局台账。

## 实际验证

在上述合并 HEAD 执行，全部退出0：

```sh
source /workspace/toolchains/activate-rustsimhub.sh
node tools/test-ble-configurator.mjs
python3 -m unittest discover -s tools/site-build -p 'test_*.py' -v
python3 configurator/tests/browser.py --output /workspace/setup-logs/configurator-cancel-latest-browser
git diff --check
```

Node 覆盖连接中明确取消、非 ready 禁止保存、取消丢回复不再提交、两笔原 ID 保留、到期拒绝及归档不复活；原合同/SSID/手势/防重测试保持通过。站点构建2项通过。

真实 Chromium 1440/390/320、键盘/无溢出、离线重开、真实 IndexedDB 并发 CAS/丢回复查询、等待 SW 有未决记录或另一窗口时拒绝更新、无强制刷新均通过。新增实际 mount 页面断言：连接中取消可用/保存禁止，明确取消仅调用一次，返回回滚证明后取消禁止。外部设备端口是明确标注的合成 Core 格式输入，不是实际 Core/GATT/认证证据。

本次恢复会话后首次浏览器检查因当前 Python 环境缺少 playwright 而未执行；在工作区工具虚拟环境安装 playwright 后，用现有 Chromium 重跑成功。没有修改或发布云环境配置。截图/日志仅在工作区，不提交生成物。

没有 Rust 源码改动，本批不重报旧 workspace 结果为此 HEAD 全仓通过。远程 CI 以 PR 精确 HEAD 为准，既有账单限制不冒充测试通过。

## 交接、未验收和来源

负责人按 PR 当前 HEAD 审查后可合入；回退本批提交恢复旧候选按钮逻辑，不删除原请求/身份元数据。生产 backend、授权/撤销、真实设备期限、取消/保存竞争与回滚证明仍由主线/D 提供并验收。新合入的独立授权模块未等同这些生产接线已完成。

未访问设备/NAS/生产数据库，没有真实短信、拨号、流量或公网业务请求，没有部署/签名/发布。B-01～32 的既有交付与具体接口/权限/发布路径前置见 member-b-usage-and-gates.md；本补丁不将离线候选、合并或本地测试计为全部验收。

沿用本仓库 AGPL-3.0-only，无新产品依赖或第三方源码复制；OpenAI Codex 辅助实现与检查，没有工时估造、改写他人作者或修改贡献数字。
