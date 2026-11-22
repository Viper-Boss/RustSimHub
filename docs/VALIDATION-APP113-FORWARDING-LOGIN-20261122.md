# 应用 113 短信转发单登录验证

应用源码 `cd9dd9611101eb8045597cdd582ad98f1429ae11`。源码归档 `dcf7a5dfe5525e1fdb5afd41ff7f461f3461c2c790186747e5f77596c0e74f12`。已确认应用清单 `782fd604e1d4be3ad8cb1379257c1539210bb069a04bdff28a2dcd7885ee182e`。

## 变更

- 已登录的所有者从同一会话查看记录、修改设置、测试通道和操作历史补发。转发页删除独立的验证按钮，历史补发按钮改为读取配置。
- Core 验证当前所有者和身份代次。普通配对客户端即使持有 `system.manage` 也不能管理转发。发送确认、写入请求唯一标记、配置代次和历史补发预览期限继续生效。

## 验证

- WSL `cargo test --locked --offline -p rustsimhub-core -p rustsimhub-forwarder -p rustsimhub-edge --lib --bins --features rustsimhub-edge/airsim-voble`。Core 两组共 251 项，Edge 75 项。转发服务的 5 项网络用例在默认 WSL 网络接口失败，改用 `unshare --net` 启用独立回环接口后转发服务 105 项通过。合计 431 项通过，3 项既有忽略。
- `cargo clippy --locked --offline -p rustsimhub-core -p rustsimhub-forwarder -p rustsimhub-edge --features rustsimhub-edge/airsim-voble -- -D warnings` 通过。Node 网页回归通过转发、补发、控制台、日常电话、会话续期 5 组。旧测试中心的模拟 DOM 缺少了导航需要的 `removeAttribute`，完整套件本轮未通过。转发专项覆盖已登录直接保存、旧后端拒绝、无权限、配置代次、回执保留和禁止重投。
- 5 个 ARM 程序的构建来源回读一致。ARM 更新器确认签名和清单，错钥匙与被修改的载荷均被拒绝。
- 用户授权上传安装。核对 112 与通话空闲后上传回读，排空后台任务，核验停写的一致性备份后准入。单次启动后校验 9 个 HTTPS 资源、SQLite 完整性、配对和设置。确认账本为 113，旧 112 回退保留。七项业务服务与两项后台定时器恢复，安装暂存已清理。

本轮实际外发仍待用户另行安排。整体验收 81.5%，行动台账 20/169。后续将签名包网页上传接到现有安装流程。
