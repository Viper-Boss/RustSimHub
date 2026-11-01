# 主线 ARM 检查点看板发布核验

- 私有看板源固定提交：`758ba93ffcdac3e8cc9470357941eb20bddf595a`，分支 `codex/board-task-owner-20261101`。
- 公开仓库提交：`2d0f275ea6e048086c3978bb4c352b3e575351c1`；仅同步既定13文件白名单，没有固件源代码、产物、凭据、私有日志或补丁。
- Pages run `41288340849` 对应上述公开提交；build、deploy、report-build-status 三个作业全部 success。
- github-pages 部署 `7470507499` 对应同一提交，最新部署状态 success，环境网址为 `https://viper-boss.github.io/RustSimHub-progress/`。
- 实际带缓存区分参数访问 index.html、progress.json、remaining.md、publication.json，四份HTTP 200且SHA-256逐一匹配发布清单；本地看板三份文件也匹配。私有证明文件为 `main-arm-board-pages-20261101-first.json`，passed=true。
- 顶层 Pages/legacy build API 同时仍显示 building；这个未收敛的字段保留如实记录，不称其返回 built。上线结论基于精确提交的三个成功作业、success部署和四份真实响应，不是仅凭push。

页面保留原81.5%、20/169验收，五人真实Git代码统计绑定已构建并完整CI通过的 `dc3aa493`。13个ARM程序尚未组包/签名/部署；人工验收不提前计入。

以上是固定快照。后续若出现新提交或PR，需重新回读，不把此次CI、产物或页面验证扩展到新HEAD。
