//! Source file observability functions
//!
//! This module provides business logic for analyzing source configurations
//! and counting lines in source files.

use crate::utils::fs::{count_lines_file, resolve_path};
use anyhow::{Context, Result};
use glob::glob;
use orion_variate::EnvDict;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use wp_conf::connectors::{ParamMap, merge_params, param_value_from_toml};
use wp_conf::engine::EngineConfig;

// Re-export types from wpcnt_lib for convenience
pub use crate::utils::types::{Ctx, SrcLineItem, SrcLineReport};

type SrcConnectorRec = wp_conf::sources::SourceConnector;

// 私有辅助函数
fn read_wpsrc_toml(work_root: &Path, engine_conf: &EngineConfig) -> Option<String> {
    let modern = work_root.join(engine_conf.src_root()).join("wpsrc.toml");
    if modern.exists() {
        return std::fs::read_to_string(&modern).ok();
    }
    None
}

fn load_connectors_map(
    base_dir: &Path,
    dict: &EnvDict,
) -> Option<BTreeMap<String, SrcConnectorRec>> {
    wp_conf::sources::load_connectors_for(base_dir, dict).ok()
}

fn toml_table_to_param_map(table: &toml::value::Table) -> ParamMap {
    table
        .iter()
        .map(|(k, v)| (k.clone(), param_value_from_toml(v)))
        .collect()
}

fn has_glob_pattern(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[')
}

fn configured_file_source_path(merged: &ParamMap) -> Option<String> {
    if let Some(path) = merged.get("path").and_then(|v| v.as_str()) {
        return Some(path.to_string());
    }

    merged.get("file").and_then(|v| v.as_str()).map(|file| {
        let base = merged
            .get("base")
            .and_then(|v| v.as_str())
            .unwrap_or("./data/in_dat");
        Path::new(base).join(file).display().to_string()
    })
}

fn validated_file_source_path(merged: &ParamMap) -> Result<String> {
    if merged.contains_key("path") {
        anyhow::bail!(
            "'path' is not supported for file source; use 'file' (with optional wildcard) and optional 'base'"
        );
    }

    let base = merged
        .get("base")
        .and_then(|v| v.as_str())
        .unwrap_or("./data/in_dat");
    if has_glob_pattern(base) {
        anyhow::bail!("'base' does not support wildcard patterns for file source");
    }

    let file = merged
        .get("file")
        .and_then(|v| v.as_str())
        .context("Missing required 'file' for file source")?;

    Ok(Path::new(base).join(file).display().to_string())
}

fn expand_source_paths(raw: &str, work_root: &Path) -> Result<Vec<PathBuf>, String> {
    if !has_glob_pattern(raw) {
        return Ok(vec![resolve_path(raw, work_root)]);
    }

    let pattern = if Path::new(raw).is_absolute() {
        raw.to_string()
    } else {
        work_root.join(raw).display().to_string()
    };

    let mut matches = Vec::new();
    for entry in glob(&pattern).map_err(|e| e.msg.to_string())? {
        let path = entry.map_err(|e| e.to_string())?;
        if path.is_file() {
            matches.push(path);
        }
    }
    matches.sort();
    matches.dedup();
    if matches.is_empty() {
        return Err(format!("glob matched no files: {}", pattern));
    }
    Ok(matches)
}

/// 从 wpsrc 配置推导总输入条数（仅统计启用的文件源）
pub fn total_input_from_wpsrc(
    work_root: &Path,
    engine_conf: &EngineConfig,
    ctx: &Ctx,
    dict: &EnvDict,
) -> Result<Option<u64>> {
    let Some(content) = read_wpsrc_toml(work_root, engine_conf) else {
        return Ok(None);
    };
    let toml_val: toml::Value = toml::from_str(&content).context("parse wpsrc.toml")?;
    let mut sum = 0u64;

    let Some(arr) = toml_val.get("sources").and_then(|v| v.as_array()) else {
        return Ok(None);
    };

    let conn_dir = wp_conf::find_connectors_base_dir(&ctx.work_root.join("sources"), "source.d");
    let conn_map = conn_dir
        .as_ref()
        .and_then(|p| load_connectors_map(p.as_path(), dict))
        .unwrap_or_default();
    let mut saw_enabled_file_source = false;

    for item in arr {
        if let Some(conn_id) = item.get("connect").and_then(|v| v.as_str()) {
            let enabled = item.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
            if !enabled {
                continue;
            }
            if let Some(conn) = conn_map.get(conn_id)
                && conn.kind.eq_ignore_ascii_case("file")
            {
                saw_enabled_file_source = true;
                let key = item.get("key").and_then(|v| v.as_str()).unwrap_or(conn_id);
                let ov = item
                    .get("params_override")
                    .or_else(|| item.get("params"))
                    .and_then(|v| v.as_table())
                    .cloned()
                    .unwrap_or_default();
                let ov_map = toml_table_to_param_map(&ov);
                let merged = merge_params(&conn.default_params, &ov_map, &conn.allow_override)
                    .unwrap_or_else(|_| conn.default_params.clone());
                let path = validated_file_source_path(&merged)
                    .with_context(|| format!("validate source '{}' path spec", key))?;
                let paths = expand_source_paths(&path, &ctx.work_root)
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("expand source '{}' files", key))?;
                for pathbuf in paths {
                    let n = count_lines_file(&pathbuf).with_context(|| {
                        format!("count lines for source '{}' at {}", key, pathbuf.display())
                    })?;
                    sum += n;
                }
            }
        }
    }

    Ok(saw_enabled_file_source.then_some(sum))
}

/// 返回所有文件源（包含未启用）的行数信息；total 仅统计启用项
pub fn list_file_sources_with_lines(
    work_root: &Path,
    eng_conf: &EngineConfig,
    ctx: &Ctx,
    dict: &EnvDict,
) -> Option<SrcLineReport> {
    let content = read_wpsrc_toml(work_root, eng_conf)?;
    let toml_val: toml::Value = toml::from_str(&content).ok()?;
    let mut items = Vec::new();
    let mut total = 0u64;

    if let Some(arr) = toml_val.get("sources").and_then(|v| v.as_array()) {
        // load connectors once
        let conn_dir =
            wp_conf::find_connectors_base_dir(&ctx.work_root.join("sources"), "source.d");
        let conn_map = conn_dir
            .as_ref()
            .and_then(|p| load_connectors_map(p.as_path(), dict))
            .unwrap_or_default();

        for it in arr {
            let conn_id = match it.get("connect").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => continue, // 不兼容旧写法
            };
            let key = it
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let enabled = it.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);

            // 支持 params_override 与 params 两种写法
            let ov = it
                .get("params_override")
                .or_else(|| it.get("params"))
                .and_then(|v| v.as_table())
                .cloned()
                .unwrap_or_default();

            if let Some(conn) = conn_map.get(conn_id) {
                if !conn.kind.eq_ignore_ascii_case("file") {
                    continue;
                }
                let ov_map = toml_table_to_param_map(&ov);
                let merged = merge_params(&conn.default_params, &ov_map, &conn.allow_override)
                    .unwrap_or_else(|_| conn.default_params.clone());

                let path_str = configured_file_source_path(&merged).unwrap_or_default();
                if enabled {
                    match validated_file_source_path(&merged) {
                        Ok(path_str) => match expand_source_paths(&path_str, &ctx.work_root) {
                            Ok(paths) => {
                                let mut source_total = 0u64;
                                let mut first_err: Option<String> = None;
                                for path in &paths {
                                    match count_lines_file(path) {
                                        Ok(n) => source_total += n,
                                        Err(e) if first_err.is_none() => {
                                            first_err = Some(e.to_string());
                                        }
                                        Err(_) => {}
                                    }
                                }
                                if first_err.is_none() {
                                    total += source_total;
                                    items.push(SrcLineItem {
                                        key,
                                        path: path_str,
                                        enabled,
                                        lines: Some(source_total),
                                        error: None,
                                    });
                                } else {
                                    items.push(SrcLineItem {
                                        key,
                                        path: path_str,
                                        enabled,
                                        lines: None,
                                        error: first_err,
                                    });
                                }
                            }
                            Err(err_msg) => {
                                items.push(SrcLineItem {
                                    key,
                                    path: path_str,
                                    enabled,
                                    lines: None,
                                    error: Some(err_msg),
                                });
                            }
                        },
                        Err(err) => {
                            items.push(SrcLineItem {
                                key,
                                path: path_str,
                                enabled,
                                lines: None,
                                error: Some(err.to_string()),
                            });
                        }
                    }
                } else {
                    items.push(SrcLineItem {
                        key,
                        path: path_str,
                        enabled,
                        lines: None,
                        error: None,
                    });
                }
            }
        }
        return Some(SrcLineReport {
            total_enabled_lines: total,
            items,
        });
    }
    None
}
