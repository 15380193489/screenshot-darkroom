# 参与改进

欢迎提交问题或改进建议。反馈时请说明软件版本、Windows 版本、复现步骤和预期结果；截图先遮挡隐私，不要上传收藏文件或个人备份。

## 提交邮箱隐私

在 GitHub 的 Settings → Emails 中找到自己的 `users.noreply.github.com` 邮箱，并在本地设置：

```powershell
git config user.email "你的 GitHub noreply 邮箱"
git config core.hooksPath .githooks
node scripts/check-commit-privacy.mjs
```

本项目的提交钩子会拦截普通邮箱；CI 会检查所有已抓取的提交。错误日志只报告数量，不输出邮箱内容。此检查不能代替 GitHub 账户本身的邮箱隐私设置，也不会自动清理旧提交。

## 代码和素材来源

可以独立实现已有工具中的通用功能。提交第三方代码、图标、字体或图片时，请说明来源、确认许可证允许使用，并保留要求的版权声明；有需要时更新 `THIRD-PARTY-LICENSES.txt`。未附许可证的公开代码，不应默认视为可复制使用。

不要使用其他产品的名称、标识或截图来暗示本项目与其存在官方关系。本项目不宣称截图整理、OCR 或桌面便签的首创性。

## 验证

提交前运行 `npm test` 和 `npm run test:native`；涉及桌面挂件与截图流程时，在交互式 Windows 桌面运行 README 中的集成验证。
