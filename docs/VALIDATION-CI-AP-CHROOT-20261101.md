# 热点安装 CI 环境修正

主线 `086b2116ff09e4d62c8c034cf668ac8739fbbc04` 的 GitHub run `41275851553`：原生音频、完整 Rust workspace、镜像/网页、应用签名和 AP 准备 CLI、Core launcher 均通过，随后在热点安装 chroot 的 10 项测试中出现 3 失败、2 错误。后续测试步骤尚未执行，不能称整套 CI 全绿。

## 证据与修正

- 3 个失败为 Edge 的 `device name unavailable`。Edge 本身已经把 `NotFound` 作为旧安装无设备名的兼容情况，只拒绝其他读取错误；不应放松生产检查。隔离模板补上可遍历的 `/etc/rustsimhub` 公共配置父目录（0755），仍不添加设备名文件，验证旧的 IP-only 安装。
- 2 个错误为复制真实 debug 程序时 `ENOSPC`。原脚本把所有 case 的 chroot 副本留到整类结束，较小的 runner 临时文件系统会被耗尽。现在每个 case 使用独立 TemporaryDirectory，并在复制前注册清理；成功、测试失败、setUp 中途失败都清理本轮，仍使用独立文件而非硬链接。
- 类级清理也在临时目录创建后立即注册，覆盖 setUpClass 后续失败。没有删除宿主机其他文件或修改真实服务。

## 本机验证

使用既有普通用户 WSL 工具链构建 Core、Edge、setup-ap、Core launcher 四个真实程序，`--locked --offline` 成功。随后仅在 WSL 临时 chroot 中以 Root 执行：

```text
python3 tools/test-setup-ap-install-chroot.py \
  --core <target>/debug/rustsimhub-core \
  --edge <target>/debug/rustsimhub-edge \
  --ap <target>/debug/rustsimhub-setup-ap \
  --launcher <target>/debug/rustsimhub-core-launcher
```

10 项全部通过，40.350 秒，0 失败/错误/忽略。另执行 Python AST 解析和 `git diff --check` 通过。完整 GitHub CI 需要本修正合并后重新核对，不把本机 10 项扩称为全套 CI。

未连接 NAS、未部署棒子、未做手机或 ARM 验收。整体实机验收仍 81.5%，人工测试放最后。
