# 离线维护身份资料

`tools/system/prepare-maintenance-seed.py`仅在NAS/构建机的全新Root0700私有目录准备每设备材料，不写实际NSHSTATE，不启动SSH，也不生成业务身份或设置码。输入是已经选定的单行Ed25519维护**公钥**及其准确文件SHA256；客户端私钥继续只留在电脑。

```sh
python3 tools/system/prepare-maintenance-seed.py \
  --maintenance-public-key /absolute/private/operator/maintenance.pub \
  --expected-public-sha256 PRIVATE_REVIEWED_PUBLIC_FILE_SHA256 \
  --output /absolute/private/new-seed
```

准备的`rustsimhub-v1/system`包含Root0600维护公钥、独立Ed25519 SSH主机身份及稳定`usb.serial`，各目录Root0700。主机公私钥由真实ssh-keygen派生核对，USB两端MAC与固件算法一致。私有manifest记录准确公钥摘要、主机公钥指纹、USB摘要/MAC及全部未安装/未验收标记；完整校验和同步后才写complete.marker。资料包含主机私钥，必须整体保密，不进入Git或通用rootfs。

错误摘要、SSH选项、多行/坏公钥、软/硬链接、错owner/权限、既有或部分输出一律拒绝。任何中断保留现场，不自动重新生成主机身份/USB serial；不能通过换目录来掩盖一次已有设备身份，后续须使用原索引选定材料。`validate_seed`重新核对完整目录形状、所有权、主机公私钥、串号和manifest，拒绝额外或修改的内容。

部署前仍需在受控离线状态副本上安装、核对旧内容保持，并验收固定主机公钥下的真实维护登录。不得直接把资料路径式复制进旧业务用户可改名的状态父目录；不能改变旧挂载根owner破坏业务。此工具不代替首启、可信业务初装、恢复演练或任何刷写门禁。

`tools/test-maintenance-seed-login.py`用于进一步验收选定身份。它从严格固定主机密钥的NAS连接读取已校验的材料，将服务器身份暂存到棒子Root私有夹具；实际客户端私钥始终由本机Paramiko使用，通过旧SSH通道访问候选ARM sshd的127.0.0.1随机高端口。独立90秒systemd时限及结束停止限制测试窗口。覆盖正确密钥、错误主机指纹、错误客户端、夹具撤销与恢复，以及选定主机身份保持。它不改现用SSH、不开放外部监听、不安装到实际状态分区；候选的挂载守卫明确替换为true，不能据此声称真实NSHSTATE或完整系统维护入口已经验收。实际结果须以新的私有报告为准，不能只根据工具存在判断通过。
