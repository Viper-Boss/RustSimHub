# 应用71离线候选准备

## 固定来源

冻结主线 `f598d792f82c42c11651f3883f978a6d71ddf124`，含已合并的NAS状态查询、条件绑定/撤销和管理页；源码归档SHA-256：`13ca163a34eab871f78634205b82fc4f61f9112b379726bc6c85fef285ef9a1c`。

本机普通用户WSL隔离网络执行正式 `tools/wsl-build-committed.sh`，逐项核对1283个归档文件，完整工作区1356通过、0失败、6项原有专用隔离测试忽略；12个ARM主程序编译、实际元数据/架构/GLIBC及独立输出hash检查通过。同源原生音频在ARM QEMU执行34项测试通过，共13个程序。复用已核验缓存，不声称空缓存重现；没有在棒子编译。

## 生成和验证

- 既有设备信任匹配的本机开发签名，未新建或安装信任、未导出私钥；生产运行feature、development_fixtures=false与“开发签名”是不同概念，不称正式生产发行。
- 应用格式2，固定6载荷，epoch71、schema6，合计22,858,865字节；清单SHA-256为 `8c13e9e6894e84d617706227cf52fde2aa97ad118a41409b2f0b1654393767b6`。
- Edge实际包含该冻结源码的完整NAS模块，Core实际包含条件绑定及状态查询操作；不能用源码存在替代载荷包含性。
- 同源热点维护包固定8目标，Core/AP/launcher安装契约与设备观测一致。二进制因构建来源变化而hash不同，不据此声称新增Root功能。
- OpenSSL验证两种签名域通过；实际ARM更新器在本机Root私有网络、POSIX临时目录执行verify-package，确认epoch71和清单digest。只执行验签，没有stage/choose/confirm、维护安装器或设备写入。
- 六项拒绝路径通过：ARM更新器拒绝错误外部公钥、错误签名域、损坏worker；Python拒绝schema5；维护包验证拒绝错误签名域和损坏Root助手。未把权限拒绝冒充篡改检查成功。

## 设备观察和边界

本轮只读预检记录设备仍为confirmed70/floor70、无候选且未尝试，七个应用服务active、Wi-Fi连接、普通蜂窝数据断开、转发PID0、无活动通话，现有开发签名信任匹配。预检不是未来安装准入，不是停机一致性备份，也不证明当前数据迁移或故障回退。

**未上传、未安装、未选择或确认候选。** 新增NAS回执导出代码在后续提交，**不包含在这个冻结候选中**；部署最新代码前应重新冻结构建，不把后续源码冒称71载荷。

私有记录保留于主工作目录 `.private/`：official-builder/main-build-summary/main-native 的20261102-f598d792报告、mainline-candidate-device-preflight-20261102-first、mainline-candidate71-f598d792-20261102-first、mainline-candidate71-verification-20261102-first，以及各独占产物目录。签名材料、设备日志和真实数据不提交。

下一步先完成主线代码，再做最新来源的维护准入、一致性备份、有限Root与应用部署、真实Owner/NAS和浏览器验收。人工测试最后，PMIC/关机继续搁置；原P0–P7 81.5%、本轮20/169不变。
