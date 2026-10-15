# 制作首次开源副本

在原开发仓库根目录运行：

```powershell
node scripts/prepare-open-source.mjs
```

默认生成 output/open-source/StillEnough/，按明确清单读取当前工作目录，包含未提交的源码和测试。目标已存在时拒绝覆盖，可指定新的目标：

```powershell
node scripts/prepare-open-source.mjs --destination output/open-source/StillEnough-next
```

导出保留源码、迁移、锁文件、当前 Logo、测试 fixture、对外文档和第三方许可；排除原 .git、生成物、账号数据、旧 Logo、设计截图、初始化提示及开发流水账。它不修改原仓库历史、不创建提交、不推送 GitHub。

脚本只允许写入本仓库 output/open-source/ 下，拒绝符号链接和常见敏感文件/文本模式；检查失败不输出敏感值。它不是完整秘密扫描或许可审计。清单与 SHA-256 保存在副本旁的 manifest.json，禁止连同整个 output 上传。

## 初次提交

按 RELEASE_CHECKLIST 完成复核后，在副本目录初始化独立 Git 仓库，首次提交只包含当前整理后的文件。GitHub 新仓库应为空，避免引入额外初始化历史。原开发仓库继续保留供回溯，不要从原目录直接推送旧分支或标签。

公开前确认仓库名/归属、私密漏洞报告设置和安装包计划。当前没有远程仓库地址，本文不填写虚构链接。

## 后续维护

首次发布后应选择公开仓库作为后续提交来源；不要把内部历史分支合并进公开仓库。需要重新导出时使用新目标，复核差异后在公开仓库提交。原始开发记录及本轮修改前的主要文档留在本地 .local-archive/，不进入发布副本。
