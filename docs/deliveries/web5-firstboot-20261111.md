# WEB5-FIRSTBOOT 交付（2026-11-11）

本包实现通用镜像的新设备首次热点准备和生产启动接线；未部署、未合并、未接触设备/真实Owner/密钥/SIM。整体仍 **81.5%，20/169，加权10.6%**，不因离线自测改变。

## 固定源码与复用

从 main `7535535bc5a95c42ff1ea70743b97be561f2049a` 建独立分支，未合文档分支、未改原 AP 八目标/合同或 Core/Edge/updater。

- 产品/装配源码：`ab874ea0fd3437fd9db63b8535a3e0124caeefca`。
- `git archive --format=tar` SHA256：`4d8477373f843dd17da0b140e073ba2bc21ad2b2674b44ee1513d99f7f1ebb6f`。
- 最后文档提交只增加本报告，不把报告 HEAD 冒充已编译应用。
- 真实 CLI 验证复用任务8已有 ARM 程序源码 `8470608781b4ab772597cef0d767e012b594f257`、归档 `efebf0bf4619f48041c1e66a310b583d987cd7d97348d3bf9c81e79a5d848dc1`。它们用于验证现有准备/验签合同，**不是本包新源码编译的 Core**。

## 生产行为

`tools/firstboot-ap/{build,verify}.py` 输出/验证显式 finite v1 材料，复用真实同源 ARM 元数据、外部公钥和已验签 format2 工厂应用。材料无设备身份、证书私钥、Owner或固定密码，始终 public_installable=false。compose 显式 `--firstboot-ap-artifacts` 安装它；build-system-image 再核验，原发行/刷写门禁保留。

原 generic 首启在身份初装前持久化新 request。原身份初装生成 Core identity/DB/setup code 和 USB/LAN TLS；新增 oneshot 随后调用原 Core AP binding、Edge TLS 和 Root AP preparation，核验 complete/anchor/source/签名/真实程序元数据及固定权限。旧设备无 request 不切模式、不清数据库、不重发认领窗口；已准备材料丢失拒绝重生。原 Root 私有六文件交接不变。

准备后才发布原 tmpfiles/Core launcher/target drop-in，并在 prepared marker 之前执行固定 daemon-reload，使排队的 Core 使用正确 ExecStart。另一个固定 activation oneshot 在 Core/app-select 后，只对 request+prepared 且准备服务成功的设备，非阻塞排队原 setup Edge/AP 两个服务；旧设备跳过时没有 AP/NM Wants/Requires。消费者额外要求准备成功。原 AP 服务仍依赖 Core 有限租约、Ready、idle/USB/NM 检查、确认/取消/撤权和回退。新代码没有 nmcli/APN、任意命令/API、Owner reset、删库或网络重试。

`--inspect` 只返回 requested/prepared/unknown 与首次失败存在性。第一错误仅保留类别，原 AP Unknown journal 不清理。USB/SSH 与 AP 失败独立。

## 实际验证

隔离容器网络 none、Root私有 tmpfs、只读工具/源码/ARM程序输入；未挂载宿主设备、账户目录或系统总线。使用临时 Ed25519 signer/Owner proof、私有 DB/root、数字 UID 985/986 和 QEMU user。PID1 inactive/reload 边界显式适配，**不称真实 PID1/开热点/Owner认领通过**。

- `python3 -m unittest discover -s tools/firstboot-ap -p 'test_*.py' -v`：**25/25**，在容器实际执行 Root 所有权、setgid、链接、源/合同/模式、旧设备、重复、中断、材料丢失、激活覆盖/重载失败等。
- `python3 tools/firstboot-ap/isolated_actual.py --programs /programs --qemu /qemu --sysroot /armroot --license /fixture-license --work /scratch`：**8/8**，真实 ARM Core/Edge/AP/launcher/updater 准备/验签；两台合成设备身份/TLS/AP材料独立；在 AP 前和两个激活文件之间中断重开；重复保持身份/setup code/合法 binding 后 DB 字节；权限错误拒绝；旧设备不变。
- 撤权覆盖：实际原离线 signed Owner bootstrap，之后 **SQL fixture** 将 Owner active=0/generation+1，验证 Owner记录/setup.used 保留。**未运行原 Auth revoke RPC 或 AP Owner claim**。
- `python3 -m unittest discover -s tools/system -p test_firstboot.py -v`：隔离 Root **7/7**。
- `... -p test_generic_state.py -v`：隔离 Root **7通过、5跳过**；缺 mke2fs/debugfs，原 NSHSTATE 实际 ext4 构建未运行。普通用户首次运行4通过8跳过，不替代 Root 结果。
- `... -p test_system_image.py -v`：**3/3**；`git diff --check` 通过。
- 最终实际 bundle SHA256 `e0d1029c4ffbd745030fffb0e38683ceb56962efbccfeafd620d233b7e391f13`；原始日志留本地 `.local/web5-firstboot/isolated-ab874ea.log`、`original-regression.log`。不提交合成私钥或原始 DB/材料。

## 实际失败与修复

最初 minimal Python 容器无 sh，未执行代码。随后 Root 测试真实发生2失败4错误：setgid 能力缺失，和 noexec tmpfs 导致合成 id stub 不执行。补 CAP_FSETID、测试 tmpfs exec，未放松2750检查；新服务也显式保留 CAP_FSETID。

实际 ARM CLI 首次运行被未提交输入守卫拒绝；随后发现 fixture 工厂目录0700阻止非Root遍历，按生产0755修复。再发现生产 `UMask=0077` 将新激活文件写成0600导致重复拒绝，补新建 pending fd 的精确 fchmod 并加入回归。最后审查发现 late drop-in 不能可靠扩展初始 PID1 事务、对已排队服务要求 Job=0 会阻塞首启；补上述固定激活服务/重载并允许**有序但未运行**的 boot jobs，仍拒绝活跃服务/PID。最终重新运行通过；未隐藏早期失败。

## 重跑与未运行

完整调用图和参数见 `tools/firstboot-ap/README.md`。Root 测试只在自有离线容器运行；实际 runner 需 Python/OpenSSL/Git/setpriv/mount/umount、QEMU/sysroot及4个同源ARM程序和原应用输入。使用 CHOWN/FOWNER/FSETID/DAC_OVERRIDE；私有 bind namespace 另需 SYS_ADMIN/SETUID/SETGID，不接入宿主 bus。普通测试会明确 skip Root 项。

本包**未构建/启动全部新源码的通用 rootfs/boot/modules/DTB/NSHSTATE 组合**，未真实 PID1 重载/启动 AP、NM/USB回退或真机认领/撤权，未生产签名/epoch、发布/刷写/网络发送。后续主线接纳后，应在同源全镜像隔离 boot 验证该 ordering，再做授权本机硬件验收；这属于验证边界，生产挂钩已提交。Cloud CI 计费执行未记通过。

补充只读装配复核：实际证明旧generic overlay不创建AP request，单独装新layer会造成无法到达的新入口。现 tree verifier 逐字节/权限核对两条已安装board/generic hook，并将它们和compose/image脚本纳入build干净输入守卫。实际ARM fixture额外插入旧hook负例，明确拒绝后恢复本次源码hook并通过整套25项+8项验证；不把旧847应用/overlay直接拼成新通用发行镜像。
