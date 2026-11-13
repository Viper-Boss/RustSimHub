# AirSIM声音整合与应用78安装 · 2026-11-13

Claude新增六个转发渠道、AirSIM协议/配对/短信/通话已核对；负责人补齐Core授权→Edge7593→媒体PCM及8k/16k转换。固定1935ed29同源1736 Rust通过/33专项忽略、12 ARM程序、音频54 QEMU、严格检查和7组网页入口通过。候选78验签/6拒绝、12恢复夹具、真实备份/回读/准入/单次启动/健康确认通过，应用78已确认，130条短信保持；普通流量断开、转发PID0。PR239按本笔明确授权正常合并，231/238随之接纳；云端计费导致零步骤未运行。真实iPhone配对/听音尚待验收，进度81.5%、20/169、加权10.6%保持。

## 来源与接纳

Claude主线已合并PR229（新增六个转发渠道）、230（音频夹具）、232～236（AirSIM协议、网关、Core/Edge和本地端口）、237（通话控制）。PR238固定5dcce7e7ec6fd4fe4ac8c930a017046f02b052b5是原生PCM单槽；PR231固定190c6b90dd42bf286e285e7eac3b7ccf35c6fb3a为应用77交接。负责人从当时main9d6ba382独立整合两者，文档冲突保留双方事实；不改作者或Claude工作区。

产品冻结1935ed29ab6d99619c42fffc59cab86c095f59d1，归档SHA256 `c5a70cd600be8e35ed91a719a0194f8d12637213e9b53b5548ad18f730583dd5`。PR239按用户本笔明确授权正常合并，最终main `53a68b95c7cfc56912db2c5ae79ea532da9536e6`与该受测Git完整树一致；231/238实际状态已回读为MERGED。合并提交不冒充相同归档标识。云端付款/消费上限导致test/native-audio零步骤；首次自动合并被拒后，获得用户明确授权才重试成功，未改保护规则。

## 实现与边界

- Core `crates/core/src/airsim_pcm.rs`：原AirSIM HMAC/重放保护、该手机自己的CallLease、待接通状态与ACTIVE分离、一次性IssueMedia、250ms续查和2秒短能力。拨号等待绝对上限90秒；非ACTIVE不打开ALSA。
- Edge `crates/edge/src/linux/airsim_pcm.rs`：仅本地地址7593、原AIRSIMREADY、8k/16k S16LE20ms转换、内部凭据不外发；本机队列/帧龄/慢写有界，撤权/EOF/监听取消关闭子连接，不保留游离泵。
- Native `native/audio-alsa/src/worker.rs` / `pcm_link.rs`：Claude的pcm.sock仅接受Edge UID，与WebRTC共用单槽，先兑换原Core许可再打开ALSA；Root与服务单元保持兼容，不新增Root常驻进程。
- Wire `crates/wire/src/airsim_pcm_tests.rs`：内部命令形状/长度/IP/票据检查；浏览器入口明确拒绝，不把浏览器当可信peer生产者。
- 对照本机NasSimHub的Go PCM实现核对Ready、拨号等待和关闭语义；沿用原Auth/Store/CallPort/媒体许可，无新增依赖或新签名域。

真实TCP+Unix合成后端测试证明握手/转换/单次凭据/撤权/断链/取消，Core测试使用真实Auth/Store与有限FakeCalls；没有把它们当成真实modem/ALSA或iPhone证据。

## 实际验证

正式 `tools/wsl-build-committed.sh` 在冻结归档上执行：workspace全特性1736通过、33专项忽略（不计通过），12个默认ARM程序构建；原生音频同源54 ARM/QEMU通过，真实--build-info和GLIBC/架构核对。fmt、workspace全特性全target严格Clippy通过，7个Node入口通过。Core默认特性测试target严格Clippy仍有4处既有feature-dependent useless_conversion告警，未修改成员代码或声称该项通过。

候选：开发信任沿用，epoch78，六载荷共23998041字节；本机Python/OpenSSL与实际ARM更新器验签通过，错误密钥/签名域/损坏载荷等6拒绝路径、12恢复夹具通过。Root维护材料仅作同源契约核对，现有helper/launcher契约匹配，实际安装未替换Root helper/正式更新器/单元。

安装顺序：新鲜只读确认77与信任/通话空闲→维护目录上传且回读→独立600秒恢复保护→暂停并持锁tar备份→本地回读/SQLite integrity_check→恢复77→重新保护下admit-stage→一次启动78→服务/7路音频/8资源逐字节/原保护数据核对→confirm78并停止timer。130条短信及原所有者保持。旧77组/备份保留；未刷分区、写NV/基带或进行PMIC实验。

开发过程的失败未覆盖：新代码Clippy的chunks转换已修；FakeCalls真实dial状态1的测试夹具已纠正；默认Core的4处既有告警仍在；本机verify包装脚本CRLF在执行前被sh拒绝后修正，恢复夹具首次用Root导致临时路径断言失败后按原非Root条件运行12项通过；维护辅助脚本的固定SSH模块日期误替换在导入时失败、未连接设备，纠正后唯一upload流程完成。所有原日志留私有目录，不修改产品断言取绿。

## 当前设备与下一步

应用78已confirmed/floor78，guard停止；9个主要服务正常，普通数据断开、转发PID0、130短信保持。手机配对/真实听音尚待验收，不把“已安装”当“声音已通过”。唯一Owner钥匙仍在原Chrome站点，未清理/导出。浏览器扩展Debugger unattached、原生窗口坐标状态错误，自动控制未成功输入，已请用户只完成配对操作。

用户已准备AirSIM iPhone。下一步保持默认8k，先配对且不拨号；观测准备后只拨10010一次、短时结束，不自动重拨Unknown。下行不等于上行，双向另用自有第二手机。随后再逐项验证断网、撤权、争用和冷启动；锁屏APNs/VoBLE/远程发现仍不支持或待资源。新六个转发渠道需各自外部目标实测；不代办未分配的成员模块。

辅助脚本、构建证明、签名候选、安装各阶段JSON、原数据备份与设备原始输出仅在主工作区 `.local/` / `.private/`，名字含airsim/78/20261113；不提交密钥、电话号码、验证码或原日志。主工作区原11项未提交修改保留。81.5%、20/169、加权10.6%及所有稳定动作/权重不变。
