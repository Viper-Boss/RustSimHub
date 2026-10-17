# 带固定音频驱动的完整离线候选

2026-10-17。组合源`075b68e0409804d39fcc6f2fb44cbb617fd13aa0`，应用/Native仍为已听音验收的`4e6fea6bf482718e4958d3541a8dee7b362f86c7`，源归档SHA`d02625f65041de8207d8d3d81c20f623c1f9681e00d64abb3f565fee928b6012`。锁定基础220包及私有开发签名factory保持原来源。新树12367项，SHA256 `c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376`，旧P6L树保持不改。

## 驱动组合和候选运行

22个音频模块从固定KyonLi/AirSimHub源码在本机重建，逐字节等于只读捕获的原系统文件。全部Root/0444复制到新候选updates层；缺件、额外文件、篡改、链接、不同架构/身份拒绝。包含Native而未明确提供驱动目录也拒绝。实现见[kernel-audio-layer.md](kernel-audio-layer.md)。

- 新树depmod后，host modprobe在禁用host配置/安装钩子的条件下解析全部22项，音频依赖全部使用updates层；四项设备树别名唯一匹配。
- `test-kernel-audio-layer.py`在只读/无网络namespace中，删除CAP_SYS_MODULE后实际运行候选自身的ARM64 modprobe；全部22个依赖图和四个别名与组合结果一致。没有执行insmod或启动驱动服务。
- 同一候选的六个生产ARM程序以UID986、Native以UID984实际运行，来源/权限一致，self-check不写硬件。systemd静态验证覆盖Core/Media/Edge、驱动服务、首启/安装/维护及NM/MM/rmtfs，退出0。仅保留原装networkd/timesyncd的两条`!!`兼容提示。
- 运行及静态检查后整树内容/mode/numeric owner/capability摘要保持；state为空，flash/release/recovery/board_boot及自动健康确认仍false。

私有证据：system-tree-p6m-audio-4e6fea6-second-{manifest,runtime,runtime-recheck,verification}.json、对应build/verify日志和exit0，以及system-tree-p6m-target-kmod-first.json passed=true。WSL树位于`/home/wzs11/.cache/rustsimhub/system-tree-p6m-audio-4e6fea6-second`。第一包装脚本意外多出`+`参数，在创建树前被argparse拒绝，first-build.log/exit2保留；修正后用独立second路径完成，不能重标首次尝试。

## initramfs与启动分区配方

已核SHA的上游ramdisk实际是zstd。只读有界解码CPIO得到551项/100个内核模块，没有Qualcomm音频模块；因此这份initramfs未包含会先于新rootfs加载的stock音频模块。不由此推断新OS已启动，也未从源码独立生成initramfs。私有kernel-source-20261017-first/initramfs-module-audit.json；初始gzip假设失败记录保留。

仅替换DTB的完整boot对照失败，保留boot-repack-comparison.json。进一步核对证明记录备份与固定上游header只在kernel_size、主cmdline和image_id有差异：主cmdline的单一root选择改为`root=LABEL=rootfs`，其余字段保持；DTB使用固定源码重建结果，按Android v0重算image_id并补零至64MiB。结果SHA256 `d5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee`，逐字节复现整个记录boot备份。

公开`tools/verify-audio-boot-recipe.py`只接受锁定上游boot和重建DTB，以内存/流式hash核对board记录，只输出JSON，不能写boot文件或设备。错误输入、重复/缺失/超长root参数、已有报告拒绝；原header不被调用侧修改。实际运行repository-boot-recipe-first.json passed=true，boot_image_written=false。

```sh
python3 tools/verify-audio-boot-recipe.py \
  --upstream-boot /private/upstream/boot.img \
  --audio-dtb /private/rebuilt/audio.dtb \
  --report /private/new-boot-recipe.json
python3 tools/test-kernel-audio-layer.py \
  --system-tree /private/new-candidate/rootfs \
  --report /private/new-target-kmod.json
```

最终55项板级隔离及34项rootfs回归通过，严格umask077，独立日志audio-module-boot-recipe-tests-final.log/exit0。没有重新编译未变的Rust应用，不将先前209项证据称为本轮重跑。

此轮证明驱动层来源、实际ARM依赖解析及记录旧boot的完整配方；没有证明新OS冷启动、掉电、物理恢复、维护交付或完整内核/initramfs独立重建。新启动守卫仅有隔离夹具和静态验证，不能称实机加载已通过。真实系统相关门禁继续保持；不新增通话/短信/普通蜂窝测试/分区写入，整体73.5/100约74%，不以构建和重复用户态探针增加进度。


2026-10-17补充：598f273完整CI通过（https://github.com/Viper-Boss/RustSimHub/actions/runs/38996792278），包含Rust测试/production检查/Clippy与板级夹具。随后对固定orig源码tar流式计算86859个文件/符号链接的Git blob、mode和目录树，1479589504字节，根tree精确为72d4e8e42649f763dd969a1de893051ed64387a1，与93a71ee提交一致；此前只核68个qcom文件的范围已补到完整源码。没有解包或执行源码。私有complete-source-tree-hash.json及.local/hash-complete-kernel-source-tree.py保留；这证明完整来源包与固定Git提交一致，不声称独立重编译了整个内核。
