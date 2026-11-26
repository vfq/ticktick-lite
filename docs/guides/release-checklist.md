---
id: GUIDE-0003
title: 发布检查清单
status: active
updated: 2026-11-26
---

# 发布检查清单

## 1. 代码

- [ ] `cargo fmt --all -- --check` 通过（本机）
- [ ] CI `CI` workflow 全绿（`fmt` / `clippy (core)` / `test (core)` / `docs`）
- [ ] `windows-check` workflow 手动跑过一次且通过（app + plugin 的真实编译）
- [ ] 无 `TODO` / `dbg!` / `println!` 残留（`rg -n 'dbg!|println!' crates plugins`）
- [ ] `Cargo.lock` 已提交且与 `Cargo.toml` 一致

## 2. 数据

- [ ] schema 版本号已递增，迁移函数幂等（对同一库执行两次不报错）
- [ ] 老版本数据库能直接打开（用上一版的 `data.db` 副本试一次）
- [ ] 导出 JSON 的 `format_version` 与结构一致
- [ ] 设置项的新键与旧键兼容逻辑有测试覆盖

## 3. 文档

- [ ] 本次改动的 REQ / BUG / ADR 状态已置为 `implemented` / `fixed` / `accepted`
- [ ] `docs/README.md` 索引表已更新
- [ ] `README.md` 的功能表与实际行为一致（历史上出现过"文档说能，代码没做"）
- [ ] `design/ui-spec.html` 与实际界面一致（若改了布局或配色）

## 4. 真机验证（CI 覆盖不到，必须人工）

- [ ] 窗口可拖动；双击标题栏行为符合预期
- [ ] 置顶开关生效，重启后保持
- [ ] 关闭按钮隐藏到托盘；托盘菜单「显示 / 退出」正常
- [ ] 设置窗口：打开 → 关闭 → 再打开（不产生两个窗口），可拖动
- [ ] 番茄钟：默认不展开；展开/收起；倒计时在标题栏
- [ ] **系统通知**：番茄钟到点弹出，应用名为「滴答 Lite」（不是「Windows PowerShell」）
- [ ] 标签：多选、改色、改名、删除、点击筛选
- [ ] 清单 chip 行在 5 个清单 + 长名下不遮挡
- [ ] 导出 JSON：选 vault → 选路径 → 文件可解析

## 5. 发布

```bash
git tag -a v0.x.0 -m "v0.x.0"
git push origin v0.x.0
gh release create v0.x.0 --title "v0.x.0" --notes-file CHANGELOG.md
```

- [ ] tag 与 `Cargo.toml` 的 `version` 一致
- [ ] release notes 里列出破坏性变更（如导出格式升级）
- [ ] 二进制从 CI 的 Artifacts 取，不本地构建
