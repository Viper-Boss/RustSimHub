# WEB3-REL：有限产品发行装配交付

记录日期：2026-11-04（Asia/Shanghai）。第4包由用户指定负责人本机接手，其余窗口业务文件未改。离线代码、构建及材料已交付；未部署或完成整机验收。

## 固定来源与真实产物

- 基线main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。
- 最终受测产品提交：`6c7da8403ae45b3aed32a415e49e7ebbcd91d1df`。后续交付记录提交不冒充重新构建过的产品来源。
- 完整Git archive SHA-256：`1f70b1bbb9ae05ac79f04c0553f792218afd007ea6f474d912141907a6b0fd96`。
- `legacy-v1`：实际13个ARM程序，`profile-build.json` SHA-256：`75ee2de4304b7d1aa23f49e45df248dd9153c62bc94d0ec81fcc77f8d69ecdee`。
- `product-email-recovery-v1`：实际16个ARM程序，清单 SHA-256：`67d38cf50f8130c693e8d3d237187d5afe01d0672a4cebcfa0a4a3b2229c104e`。

两套全新独立target实际编译，不将13+16称为29种程序。五个有限profile均有解析/材料测试，完整ARM构建只对上述两套执行。产品套另含真实受管Go SDK、NKN Root与Rust BLE host；SMTP是Core/forwarder编译特性。NKN不冒称全Rust；音频是默认LAN构建，没有宣称TURN媒体正式启用。

清单始终为 `signed=false / epoch_assigned=false / installable=false / bootable_rootfs=false / health_authorized=false`。不是updater签名候选格式，没有boot/DTB/initramfs整机发行材料。

## 十步交付对应

| 原动作 | 实际实现 | 边界/接续 |
|---|---|---|
| 1 盘点与profile | 五种固定枚举，普通outbox3/worker4，邮件outbox4/worker5，独立A路径BLE host | 不混用D服务协议 |
| 2 实际构建 | `tools/product-release/release.py`核完整来源、逐程序ELF/摘要/真实元数据，Rust1.98 locked/offline、Go1.27.3 readonly/offline | 默认13、产品16通过，未全量编译其余三种 |
| 3 有限解析 | 四字段Root安装profile绑定Core字节/来源；未知项、摘要/契约不符拒绝 | 不接受任意命令/路径/UID |
| 4 launcher与服务 | 实际Core前缀探测后同PID exec；无profile保留原默认；固定drop-in、BLE/NKN单元、目录/总线策略 | PID1/cgroup/mount/BlueZ未实测 |
| 5 系统角色 | 复用setup-ap真实身份守卫，比较local/NSS快照，拒绝独立BLE/NKN角色冲突、别名及通用IPC扩权 | 本次合成快照，未创建真实账户 |
| 6 所有权接线 | provider编译与开启分离，材料无Install/enable链接/ON标记/凭据；转发和NKN三引用沿用原监督器，不重写业务 | 构建不启进程；没有实际共享SDK/邮件关闭验收 |
| 7 三方兼容 | `compare.py`调用真实ARM launcher，保留0/3/4、worker/outbox/回退规则；base6不能批准schema9 | 真实schema7/9专用执行与数据库准入另做，未迁移 |
| 8 发行树 | 同源程序、材料、实际许可证文本/摘要、版本/依赖清单、安装回退前置；失败不覆盖 | 新目标未纳入旧Root白名单，不可安装 |
| 9 隔离验证 | 下表实际Rust/Python、UID临时root、ARM比较/拒绝、离线单元/私有总线 | 健康待窗口3，不用裸confirm冒充自动确认 |
| 10 交付 | 本报告、[使用说明](../release/product-profiles.md)、分阶段代码及独立PR | 主线接纳/跨包整合与设备验收单列 |

## 最终源码验证

| 检查 | 实际结果 |
|---|---|
| launcher Rust | 35项通过，严格Clippy/native build通过 |
| 产品工具Python | 18项通过，含失败/超时/日志超量进程组回收、Python优化拒绝、空Go sum/模块闭包 |
| ARM清单工具Python | 14项通过，含有界stdin与明确0/3/4返回码 |
| 真实UID临时root | 15项通过；本机隔离网络/chroot，实际launcher权限及exec路径 |
| ARM三方比较 | 7种实际CLI结果符合预期，授权字段始终false |
| ARM角色拒绝 | Root/SDK/BLE三项无身份或继承FD的预检拒绝，未连公网/设备 |
| 单元材料 | 两份单元离线解析通过，没有PID1启动 |
| 总线策略 | 私有D-Bus中1项允许、4项拒绝，没有真实BlueZ/射频 |
| 完整构建 | 两套ARM构建、全部程序摘要/材料/许可清单核对通过，构建后提交源码字节保持 |

主要复现命令（准备固定缓存和输入，详见使用说明）：

```sh
cargo +1.98.0 test -p rustsimhub-core-launcher --locked --offline
cargo +1.98.0 clippy -p rustsimhub-core-launcher --locked --offline --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tools/product-release -p 'test_*.py' -v
python3 tools/test-arm-build-manifest.py
# 本机临时root、隔离网络，不是设备命令：
unshare --net python3 tools/test-core-launcher.py --launcher /private/native/rustsimhub-core-launcher
# 两次分别选legacy-v1和product-email-recovery-v1，NEW_OUTPUT必须不同：
python3 tools/product-release/release.py --profile PROFILE \
  --archive /private/source.tar --repository /private/linux-checkout \
  --revision 6c7da8403ae45b3aed32a415e49e7ebbcd91d1df \
  --source 1f70b1bbb9ae05ac79f04c0553f792218afd007ea6f474d912141907a6b0fd96 \
  --roles /private/roles.json --verified-sdk-stage /private/verified-sdk-stage \
  --sysroot /private/gnu/usr/aarch64-linux-gnu --audio-sysroot /private/alsa-sysroot \
  --output /private/NEW_OUTPUT
python3 tools/product-release/compare.py --launcher /private/programs/rustsimhub-core-launcher \
  --sysroot /private/gnu/usr/aarch64-linux-gnu --request /private/versions.json
```

角色、数据版本和比较请求为合成输入；程序build-info为实际ARM输出。7种比较为普通兼容、邮件4拒绝旧回退、显式维护、现有邮件4拒绝旧worker、worker协议不符、base6拒绝schema9、缺worker需维护。没有启动schema7/9的真实专用业务模式。上述组不相加为整体固件测试数，不宣称全仓回归/云端CI通过。

原始日志、完整产物及可复核JSON仅保存在本机私有证据目录，统一前缀 `product-`、日期 `20261103`、最终来源 `6c7da840`；不把个人路径、身份或构建材料提交公开看板。

## 中间失败保留

1. Windows worktree的绝对Git路径不适用于WSL；改用同对象库私有Linux checkout和完整固定archive，原脏目录保持。
2. raw来源首次未通过原始长度/摘要门禁，没有采用；后来取得31个精确Git blob，逐一通过原blob/SHA-256/长度门禁，旧失败保留。
3. 653d0c7产品构建将合法空 `go.sum` 当空程序拒绝。只让源码读取允许空文件，软链接/非常规/多链接仍拒绝，程序非空限制不变。
4. 0758141实际16程序已编译，但许可检查错误要求链接全部历史模块。修为仅允许已知模块子集，继续拒绝新模块/变更版本或许可；ARM纯Go未链接 `golang.org/x/sys`，清单明确列出。许可收集不等于安全公告清零或依赖正式批准。
5. 7581a2e曾通过两套构建；随后补有界编译进程组回收，在最终6c7da840重跑通过。旧失败记录未覆盖。

## 后续真实接续

窗口3健康协议尚未装配，当前没有自动确认profile；交付后按完整新来源接续。邮件3→4回退、schema9需要真实有限准入/迁移计划，不能凭通用build-info或编译成功批准。Root目标需要独立固定写入接纳；本包不改旧八目标表或签名载荷格式。真实mount/cgroup、共享SDK/关闭、BLE射频恢复、邮件递送、冷启动均未验收。

本轮未签名/分配epoch、上传/安装、访问设备/NAS、拨号/短信/通知、连接公网NKN/TURN、修改真实NSS或启用服务。整体验收81.5%、20/169、加权10.6%保持，不将离线交付加成整机验收，也不重算分母/成员贡献。
