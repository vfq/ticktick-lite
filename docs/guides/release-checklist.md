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
- [ ] 本机 `cargo check --workspace --all-targets` 与 `cargo check -p ticktick-app --no-default-features` 均 0 error 0 warning（CI 覆盖不到 app/plugin，见 ADR-0002）
- [ ] 可选：`gh workflow run windows-check.yml -f full_build=true` 拿 Windows 产物。**该 workflow 跑在 GitHub 托管 runner 上，本账户因计费无法分配时会长挂 queued**，此时改用本机 `cargo build --release`
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
- [ ] 关闭按钮隐藏到托盘；**设置窗口同时关闭、不残留**；托盘菜单「显示 / 退出」正常
- [ ] 设置窗口：打开 → 关闭 → 再打开（不产生两个窗口），可拖动
- [ ] 设置改动即时生效：改「显示已完成」主窗立刻变化；改番茄钟时长后下一次 Idle 按新值计
- [ ] 番茄钟：默认不展开；点标题栏 🍅 展开/收起；收起后计时不断、标题栏显示 `MM:SS`
- [ ] **系统通知**：番茄钟到点弹出，应用名为「滴答 Lite」（不是「Windows PowerShell」）；关掉 `pomodoro.notify` 后不再弹
- [ ] 标签：多选、改色、改名、删除（任务保留）、点击 chip 筛选、筛选条可移除
- [ ] 新增同名标签报「标签「x」已存在」，不产生重复行
- [ ] 清单 chip 行在 5 个清单 + 长名下不遮挡
- [ ] 导出 JSON：选 vault → 选路径 → 文件可解析，`format_version == 2` 且 `tags[].color` 为 `#RRGGBB`

## 5. 发布

```bash
git tag -a v0.x.0 -m "v0.x.0"
git push origin v0.x.0
gh release create v0.x.0 --title "v0.x.0" --notes-file CHANGELOG.md
```

- [ ] tag 与 `Cargo.toml` 的 `version` 一致
- [ ] release notes 里列出破坏性变更（如导出格式升级）
- [ ] 二进制从 CI 的 Artifacts 取，不本地构建
