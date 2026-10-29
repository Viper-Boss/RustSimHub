# 本仓库分支协作与访问状态

2026-10-30补记：成员B当前账号改为`wnfhsj88`，已重新回读确认本仓库权限为Write。未授予Admin、未改变main保护、未重写Git历史；旧账号权限本轮未撤销。以下2026-10-27记录保留当时语境。

2026-10-27，用户明确要求四名成员直接在 `Viper-Boss/RustSimHub` 操作，不使用个人fork交付。目标权限为Write，功能分支提交PR，由负责人审核。

| 成员 | 已核验状态 | 接下来 |
|---|---|---|
| fungi1220 | 已有Write邀请，本轮回读确认，待接受 | 本人接受邀请后推送本仓库功能分支 |
| yh1030 | Write已生效 | 可直接推送本人功能分支并提交PR |
| wnfhsj | 最终回读Write已生效 | 可直接推送本人功能分支并提交PR；此前主页404原因另核 |
| RedamancyGTW | 新Write邀请已创建并回读确认，待接受 | 本人接受邀请；公开主页404原因另向GitHub核实 |

邀请待接受和权限已生效分别记录。没有授予Admin，也没有替成员接受邀请。仍待接受的两位成员可登录自己的GitHub查看[通知](https://github.com/notifications)，或访问[本仓库邀请页](https://github.com/Viper-Boss/RustSimHub/invitations)。无需向负责人共享密码、令牌或SSH私钥。

两位主页404的账号均已被GitHub邀请接口识别；这不能证明公开可见性异常的原因。GitHub说明账号在部分临时限制下仍可登录、但主页或搜索不可用，实际原因以账号提示/邮件及官方支持回复为准。[GitHub官方说明](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/preventing-unauthorized-access#account-is-restricted-after-suspected-compromise)

## main审核规则

任务书要求：个人功能分支 → PR到main → 负责人核对完整HEAD和测试后合并；成员不直接推送main，不强推主线、不删除主线、不覆盖他人分支。

2026-10-27 已通过 GitHub API 启用并回读 main 保护：至少1个批准、要求 Code Owner 审核、新提交使旧批准失效、讨论须解决、既有 `test` 和 `native-audio` 检查须通过（绑定实际 GitHub Actions App）。禁止强推和删除 main。`.github/CODEOWNERS` 指定 `@Viper-Boss`；该文件随本次提交同步后，负责人审核规则才具备完整代码归属文件。

保留原有管理员维护入口（`enforce_admins=false`）；这不是给成员授予管理员权限。功能分支之间的协作边界仍按任务书执行，main 保护不会为每个人锁定所有其他功能分支。

最初两次保护请求被 GitHub 以参数格式422拒绝，没有生效；改用接口接受的单一 `checks` 字段后成功，最终设置已重新读取确认。权限/保护的原始回读只保存在私有证据。

实际状态复核记录仅公布账号、权限与邀请状态；令牌、原始管理响应和邀请ID留在私有维护证据。登记账号、获得权限、交付分支及任务验收是四个独立状态，不互相替代。
