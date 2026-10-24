# P4H 独立配网材料、证书和可信导出

2026-10-24。新增AP专用Edge准备模式、Root材料准备和私有可信导出，配套独立服务候选模板。完整309项Rust测试、严格Clippy、Core生产检查、两程序及库ARM64构建通过；真实Edge/Root命令在Linux临时目录完成两项联合测试。本轮未访问/安装棒子，未启动服务或热点、未切网/拨号/发短信、未启用普通数据或修改系统证书信任。整体仍79.5%。

## 实际可用的本地命令

以下是可信本地安装器的接口，不是网页可提交的路径或任意Shell入口。目录、UID及设备绑定由安装器提供。

```text
rustsimhub-edge --prepare-setup-local PRIVATE_EDGE_PARENT
rustsimhub-setup-ap --prepare-material PRIVATE_ROOT_PARENT PRIVATE_BINDING_DIR EDGE_INSTALLATION EDGE_UID
rustsimhub-setup-ap --export-material-trusted MATERIAL_INSTALLATION NEW_PRIVATE_EXPORT_DIR
```

1. Edge命令以独立Edge用户准备700目录下的installation，生成AP地址专用证书，固定精确Origin为https://172.31.85.1:8443，监听loopback并额外监听固定AP地址。没有通配地址、媒体工作进程或legacy接口，不绑定TCP、不改现有日常Edge安装目录。
2. Root材料命令验证Root私有binding.json、独立Edge安装记录和证书摘要。绑定格式1只接受realm、core_uid、device_public_key；Core UID必须匹配Edge配置，当前板级配置986、Edge用户985。真实绑定的导出/首次本地窗口生产者仍需下一阶段接入，不能用测试值安装设备。
3. Root生成每设备随机SSID、48位十六进制口令和UUID，将其连同真实设备绑定及证书副本放入Root的installation。配置、证书、记录均0600，父目录0700；证书摘要、材料摘要和输入绑定/Edge配置摘要相互核对。普通输出只有准备结果及摘要。
4. 可信导出只写一个新建的Root私有目录，包含connection.json和公开证书cert.der。连接文件含SSID/口令、固定地址、真实设备公钥十六进制及摘要、证书摘要，可由后续USB/标签交付使用；秘密不打印到标准输出，不提供HTTP下载，不修改系统信任库。

材料导出不含首次认领的一次性代码，也不替代所有者证明。当前还不能据此声称手机首次配网已完成。

## 重复执行与异常

Edge沿用既有完整安装记录和原子发布规则；Root使用独占文件锁、完整pending校验、Linux RENAME_NOREPLACE发布和目录同步。完整pending可继续发布原材料，残缺/冲突pending会保留并拒绝，不重生成一套口令掩盖中断。

既有材料只能精确重验。设备绑定、Edge配置、证书或材料发生变化，即使只是输入原字节改变，也拒绝自动迁移；原数据保留。拒绝软链接、弱权限、错误UID/GID、非单链接文件、超长文件及旧目标覆盖。可信导出先验证材料记录，损坏时不创建秘密副本；已有导出目标不覆盖。

准备命令不读Edge私钥；真实私钥持有和AP SAN/有效期/精确身份仍由P4G运行时HTTPS握手验证。静态准备成功不会发布Root Availability，只有实际服务检查通过才可报告就绪。

## 服务候选及未安装边界

模板在firmware/system/setup-ap/，位于正式overlay之外，未复制到镜像、未启用，也没有[Install]自动启动目标。AP Edge仍为无能力的独立用户，Root消费者只保留NET_ADMIN，分别限制文件写入和地址族。tmpfiles模板包含P4F保护输入/输出目录。

这些模板尚未进行受限服务实机验收，也未接镜像构建、Root伴随程序的固定来源安装、Core显式AP模式或Core日志目录准备。不能只复制模板或启用服务就当作完整安装。现有Core、日常Edge和应用恢复预算均保持。

## 已验证

- 309项工作区Rust、严格Clippy和生产Core检查通过；AP材料/执行包32项测试。新增Root材料6项、Edge准备2项，覆盖幂等、独立随机值、完整pending恢复、残缺/链接/权限拒绝、精确身份和损坏拒绝、私有导出及不覆盖旧目标。
- 两项真实CLI联合测试在独立Linux临时目录中，以普通用户生成Edge材料，再以Root导入、重复核对并导出。核对Root文件所有权/0600、秘密不在输出、准确证书副本和公钥；错误权限/更改绑定拒绝且原材料保持。未运行--serve，无网络或无线操作。
- ARM64 Edge、执行程序及配网库已构建并检查机器类型183；开发产物未签名，未嵌入冻结源码包证明，未用于部署。
- CI增加真实CLI夹具步骤，在工作区构建之后运行。精确提交的远端结果另存；不把之前P4G提交成功当成本轮成功。

私有证据使用setup-ap-material-20261024-fourth.log/.exit、setup-ap-material-cli-20261024-second.json、setup-ap-material-arm-20261024-second.json及setup-ap-material-checkpoint-20261024-first.json。第二轮构建的sudo需要交互认证，已保留退出1日志；改用WSL显式Root只执行临时目录夹具，两项通过。第三轮后补导出真实公钥，第四轮为最终验证；没有通过改权限或抹去失败记录获得通过。

## 下一步

1. 从真实Core取得可信realm/设备绑定，接未认领设备的本地一次性窗口和代码交付；保留setup.used，不重建所有者。
2. AP到STA的试连接、确认、错密码/超时恢复AP及隔离规则交接，补网页入口；现有USB条件尚未解除。
3. 固定来源的Root/Edge/Core联合安装及模板隔离验收，再在具备备用维护入口、备份和人工在场时实测热点/手机首配。
4. Root Unknown资源的明确核对工具及断电/重启矩阵，继续保留原有全目标范围。

当前不需用户配合；上次确认业务185ec6e/confirmed43和67短信保持为最后实机基线，累计10010仍44，关机调查继续搁置。

源码入口：crates/edge/src/linux/installation.rs、crates/setup-ap/src/executor/provision.rs及其tests.rs，tools/test-setup-ap-material.py。
