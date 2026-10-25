# X1J：签名应用组纳入原生转发程序

2026-10-25。本轮完成签名格式、实际 CLI 和离线系统组合接线，尚未签名生产候选、安装服务或访问棒子。原 P0–P7 仍 80%；X1 单独跟踪，安装和数据回退仍未完成。

## 两个固定文件组

manifest format1 保持原五项：Core、simhubctl、Edge、Native 音频、Opus 许可证。新增 format2，在许可证之前加入 `rustsimhub-forwarder`，其余名称和顺序固定。不能增加任意程序、改变顺序、缺少 worker、重复名称或使用目录穿越路径。程序继续要求 Linux ARM64 ELF，单文件及整个组仍限制 24 MiB。

签名 envelope 保持 `RustSimHub.AppManifest.v1\0` 域，覆盖 manifest 的精确字节，包括 format 字段；不是将 format1 的五文件签名套在六文件组上。旧更新器不支持 format2，新更新器同时严格验证两种格式。业务 schema6 和 outbox format2 均未在本轮改变。

更新器的验证、暂存、整组选中、确认和回退均按已验证 manifest 对应的固定列表处理。版本目录还必须恰好包含签名列出的载荷及 manifest/signature，拒绝在旧五文件组中夹带未签名 worker。不能只校验旧五项，再执行额外放进去的程序。运行时发布时程序改为 0555，许可证为 0444。

## 实际工具接线

- `rustsimhub-update make` 保持旧格式；`make-forwarding` 明确生成新格式。`--build-info` 声明两格式与转发组支持。仍要求本地 Root、固定输入和已有签名权限，不是网页任意签名接口。
- Python `application_package.py` 独立严格核对同样的两种名称、顺序、类型、签名、摘要、ARM64 和总大小。布尔值不能冒充整数格式。
- 开发签名打包器新增 `--with-forwarder`，默认保持旧格式；要求制品记录声明支持新组，并核对 forwarder 的同源构建摘要。仍只在 `.private` 产生开发密钥/候选，明确不是生产发行密钥。Linux 使用本机 cryptography，不再导入本地 Windows 专用 wheel。
- 固定提交交叉构建脚本会复制 forwarder 并记录摘要和支持标记。系统组合器在 format2 时额外核对同源 worker 和更新器能力；实际复制 helper 要求全新的 factory-app 目标，设定权限，再核对复制后的包和原公钥。不启用 worker、开关或生产 polkit 规则。

## 验证

436 项 workspace Rust、严格 Clippy、生产 feature 检查通过。本轮新增签名格式、完整整组选中/回退/确认、损坏 worker 和夹带额外文件拒绝用例。原格式相关用例保留并通过。

159 项系统工具测试通过，包含 1 项原有跳过；36 项 rootfs 测试通过。新系统组合用例实际发布两种签名组，核对 worker/许可证权限、复制后重验、禁止重复目标及损坏 worker 在创建 factory 路径前拒绝。

`tools/test-signed-application-formats.py` 运行真实生产更新器 CLI：临时密钥、两种真实签名、Rust/Python 互验、六文件候选选中后五文件文件组回退、损坏 worker 不改变 floor/slot，以及旧版本夹带未签名 worker 拒绝。载荷是合成 ELF 头，**从未执行这些载荷**。这只证明应用文件组回退，不证明业务/outbox 回退。

`tools/test-development-application-packager.py` 运行实际打包器，产生并核验旧/新各三个临时开发签名 epoch，核对文件集、摘要、开发来源标记和坏 worker 在生成密钥前拒绝。WSL 9p 将 chmod(0444) 报告为0555，本地夹具如实报告不支持精确许可证执行位；仍要求源文件无写位，精确设备权限已在 POSIX 临时目录的真实更新器选中路径核验。没有放松设备安装权限。

生产更新器 ARM64 构建、ELF/摘要和 QEMU `--build-info` 通过，声明格式 `[1,2]`、schema6，无系统 OTA 支持；此制品尚无冻结 source archive 绑定或签名，不能据此当作生产安装包。完整已提交源码的 CI 按精确 SHA 另存。

私有入口：`.private/forwarding-signed-group-checkpoint-20261025-first.json`。完整 Rust `check-first`、系统/rootfs `python-third`、实际 CLI 和打包器 `second`、ARM `first` 日志同前缀。

原失败保留：第一次给整套系统测试额外加网络命名空间，旧 ext4/udev 夹具在 `udevadm trigger --settle` 超时；只读核对确认其隔离网络及自身合成 loop，使用 pidfd 清理唯一验证过的遗留子进程，映射已释放。按原 CI 环境保留夹具自身挂载隔离后通过，没有停止系统 udev 或跳过用例。打包器首次权限断言失败也保留，另做 9p 模式探针确认其文件系统限制。

## 安装前仍需做

一致性备份/回读、outbox format2 的维护回退方案、专用 UID/GID 冲突检查和正式服务安装。首次格式迁移不能仅凭文件组回退测试就启用新 Core 的转发管理；需要先明确何时确认新应用、何时开放队列、哪些备份随版本恢复。现有 Owner、短信、Unknown、SIM 身份和恢复预算必须保留。

随后才做棒子实际默认 OFF、开关、合成递送、断网/撤权及冷启动验收。用户真实第三方目标、邮件等通道和历史补发仍另列。无新短信/电话，无普通蜂窝数据测试，PMIC 继续搁置。
