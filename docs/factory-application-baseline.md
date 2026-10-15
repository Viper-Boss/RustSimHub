# 签名应用首次安装基线

`rustsimhub-update install-factory STORE RELEASE_PUBLIC SIGNED_DIRECTORY`是Root安装器显式执行的本地命令。它验证现有五文件格式的签名、板型、schema、ELF架构及payload摘要，再建立factory基线。release私钥留在构建机；不生成密钥、不写分区、不处理身份或短信。

空Store先完整stage并同步，再发布confirmed；中断后只允许相同的完整候选续作。不同候选、签名错误、payload损坏、不完整slot拒绝。留下不完整slot需维护检查，不能自动删除或重置台账。

存在confirmed后仍验证输入factory包，但保持confirmed、floor、candidate、attempted，核验当前confirmed并返回其路径。较旧的不可变factory不能覆盖已更新版本或消耗pending update的一次启动机会。更新仍用stage→choose→业务健康检查→confirm；install-factory不能代替更新健康确认。

缺失state.json时只允许空versions，或从完整有效且匹配签名公钥摘要的state.next恢复。已有slot却丢失/撕裂台账时拒绝，不能静默回到floor零。Root删除全部安装状态的攻击不在该文件完整性措施覆盖范围内。

新增四项测试覆盖幂等安装、旧factory保持新版本及pending状态、两个续作阶段、不同包拒绝、丢失台账/完整next恢复、签名/payload及不完整slot拒绝。组件共9项及clippy通过。

2026-10-15实机验收：更新器源a10a492da8c1ce95a0785f3cb1d84905344e9dc3，精确归档SHA256 1dbc6775921542498df8fc4058e6f0259b04c7b93378ad3395e978074fbebfb3；本机WSL181项测试、六程序ARM64生产构建及CI38749275454成功。应用五文件则来自ca596a7/8bdf8f...与同源Native及许可，验证器与应用来源分别记录。

`tools/test-factory-install.py`在已核验候选RootDirectory、独立可写目录、私有网络/设备和空cap中执行，临时开发签名私钥仅驻本机内存，绝不上传。首装后choose、pending未尝试/已尝试状态均不被重复factory修改、再次choose回退且floor=2、低epoch拒绝、第三版确认后旧factory返回第三版、移走台账后拒绝、完整state.next恢复且floor=3全部通过。此处确认是选择器测试步骤，不代表生产业务健康检查或真实掉电。

私有报告`.private/factory-install-a10a492-second.json`passed/cleanup_verified=true；旧四服务active、PCMclosed，没有新拨号/短信/蜂窝或分区写入。首轮失败记录保留：systemd-run默认展开`${third##*/}`导致确认参数为空；纯合成对照确认DEFAULT为空、`--expand-environment=no`时为expected。新脚本关闭额外展开，完整新目录重跑通过。另有WSL Root首次预检因仓库所有者检查拒绝，未编译，改用既有wzs11构建账户成功，不放宽Git全局信任设置。

尚未完成正式启动组、生产健康确认、可信首次配对/TLS交付、应用远程更新或整机恢复，不因本轮首装命令单独增加进度点数。
