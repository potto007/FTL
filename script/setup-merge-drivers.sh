#!/usr/bin/env bash
# 注册 FTL 合并驱动，同时兼容历史检出中的驱动名称。
# 只配置当前仓库，不移动文件或改动旧应用数据。
set -euo pipefail

for driver in ftl-ours openwarp-ours zap-ours; do
    git config "merge.$driver.name" "Always keep FTL version (custom driver)"
    git config "merge.$driver.driver" true
done
git config rerere.enabled true
git config rerere.autoupdate true

echo "FTL merge drivers + rerere configured."
echo "  rerere.enabled        = $(git config --get rerere.enabled)"
echo "  rerere.autoupdate     = $(git config --get rerere.autoupdate)"
echo "  merge.ftl-ours        = $(git config --get merge.ftl-ours.driver)"
