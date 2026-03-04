# update-changelog

更新 `CHANGELOG.md` 的标准流程（本仓库专用）。

## 自动化命令

```bash
# 仅检查一致性（不改文件）
scripts/claude-skill run update-changelog --check

# 自动修正 Unreleased 标题并补缺失 tag 段落骨架
scripts/claude-skill run update-changelog --write
```

## 核心约定

1. **`version.txt` 是当前开发版本唯一事实来源**  
   顶部 Unreleased 必须写成：`## [<version.txt> Unreleased]`。
2. **发布段落以 git tag 为准**  
   例如 `v1.17.3-alpha`、`v1.17.4-alpha`、`v1.17.5`、`v1.17.6` 都应在 changelog 中可追溯。
3. **分类固定**：`### Added` / `### Changed` / `### Fixed` / `### Removed`。

## 工作流程

### 1) 收集版本与 tag 信息

```bash
# 当前开发版本（用于 Unreleased 标题）
cat version.txt

# 按版本顺序列出 tag（含 alpha）
git tag --list "v*" --sort=version:refname

# 最近发布 tag（用于 Unreleased 改动区间）
git describe --tags --abbrev=0
```

### 2) 核对 CHANGELOG 覆盖度

```bash
# 查看当前 changelog 头部
sed -n '1,120p' CHANGELOG.md

# 对关键 tag 查看提交时间与主题
git show -s --format="%h %ad %s" --date=short <tag>
```

检查两件事：
- 顶部是否是 `## [<version.txt> Unreleased]`
- 已存在的 tag（特别是 alpha）是否在 changelog 里有对应版本段落

### 3) 分析变更来源

```bash
# Unreleased 内容来源：latest-tag..HEAD
git log <latest-tag>..HEAD --oneline

# 某发布版本内容来源：prev-tag..tag
git log <prev-tag>..<tag> --oneline
```

### 4) 写入规则

- 新改动写入顶部 `Unreleased`。
- 缺失的历史版本段落按时间倒序补齐（放在 Unreleased 下方）。
- 每条用统一格式：`- **模块名**: 改动描述`（英文描述，简洁）。
- 不改已发布段落的原有语义；仅补缺失段或修正明显版本错配。

## alpha / 正式 tag 处理

- 若 `-alpha` 与正式 tag 指向同一 commit（如 `v1.17.5-alpha` 与 `v1.17.5`）：
  - 默认保留正式版本段（`1.17.5`）作为主记录；
  - 如用户要求，可额外增加 alpha 段并注明同 commit。
- 若 alpha 与正式版不是同一 commit，则都要有独立段落。

## 完成后检查

```bash
git diff -- CHANGELOG.md
```

快速检查清单：
- [ ] Unreleased 版本号与 `version.txt` 一致  
- [ ] 最新 tag 到 HEAD 的改动已进入 Unreleased  
- [ ] 所有应存在的历史 tag（含 alpha）在 changelog 可找到  
