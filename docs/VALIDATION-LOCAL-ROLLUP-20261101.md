# 2026-11-01 本机旧修复收敛

基于main 527257d，仅接回旧#34/#37/#50中的四个产品/测试提交，使用cherry-pick -x保留原作者和来源：17a63d5、07a9839、adb7bb6、6078a37。不接旧全局进度、分工、schema或旧网页，以免覆盖当前成员成果。

内容：退休短信对象回复不得写入新收件箱；实际Store内容边界与提交恢复样例；跨SIM通话来源碰撞拒绝；灯控FIFO在打开前拒绝及有限输入样例。

首轮回归在旧灯色夹具失败：该夹具仍要求蓝/青/绿及旧来电三色。保留当前main已由用户确认的黄紫/红紫/组合红黄紫方案，只更新旧夹具预期，不改变产品灯色或期限。

实际普通用户WSL离线：`cargo test -p rustsimhub-storage -p rustsimhub-core -p rustsimhub-status-led --all-features --locked --offline --quiet`通过349项、0失败/忽略；fmt及同三包all-targets/all-features严格Clippy通过。未运行全workspace、Root专用矩阵、ARM或设备；没有短信/拨号/网络/签名部署。不得把此结果当作原SMS/电话/LED整项验收。

合并成功后旧#34/#37/#50可按本批新PR替代关系关闭，不删除分支。尚未合并时不能宣称旧修复已经进入main。其他草稿仍保留。
