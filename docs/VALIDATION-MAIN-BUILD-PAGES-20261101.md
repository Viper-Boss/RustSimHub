# 主线构建收口看板发布验证

- 看板数据来源提交：`c2152bc89782015ead3d6882c00a9292bf65120d`。
- 五人代码统计提交：`c64e52dc414ff02000a7628e118099b01fcbf7d7`；固定ARM/音频构建基线另为`5d655331f61af4714eff2337a2a19d4f2a6c438e`，不将后到#160混入其验证。
- 公开看板提交：`b56c5ed6cce4dd74feec9ab0ff3aa344994d346e`；只导出既有13项静态白名单，固件仓库仍私有。
- [Pages run 41295725433](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/41295725433)的build/deploy/report-build-status全部成功；部署7471492882实际success，Pages API同时回读built且commit正确。
- 实际HTTPS读取index.html、progress.json、remaining.md、publication.json，四个哈希均与该提交的publication清单一致；本地看板三个文件也逐个一致。私有原始证明在`.private/main-cleanup-board-pages-20261101-first.json`，不提交。
- 原估算81.5%、完整行动20/169不变；本轮17合并/14旧PR关闭，快照时仅剩10草稿。D#154归GTW、不改Git作者。页面重建、28项进度工具测试、Node计算/过滤/归属检查、JS语法与diff检查通过。

发布时固定5d的全套固件CI仍运行中，已通过原生音频；后续回读到AP隔离安装阶段，不能因本机1351项/13程序通过或Pages成功宣称固件CI全绿。下一轮跟踪CI并为含#160的新主线另建产物。未生产签名或部署、没有人工/设备验收增分。当前文档为发布后的证明；publication.json来源继续固定c2152bc，不因追加此文档而重新标注旧页面来源。
