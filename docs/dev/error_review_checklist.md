# 错误处理 Review 清单

本清单用于 `wp-motor` 相关代码评审，快速判断错误处理是否符合工程规范。

配套设计文档：

- [设计文档：错误体系设计](../design/error_system_design.md)

## 1. 先判断代码位置

评审前先判断该代码属于哪一层：

- 配置加载层：`wp-config`
- 业务辅助层：`wp-cli-core`
- 应用归一化层：`wp-proj`
- runtime 核心层：`wp-engine`
- CLI 展示层：`facade/diagnostics`
- 测试 / 一次性工具 / `main`

只有测试、一次性工具和极薄的入口层，才允许宽松使用 `anyhow::Result`。

## 2. 必查项

### 2.1 返回值是否合理

- 核心公共接口是否错误地返回了 `anyhow::Result`
- 领域边界是否应返回 `RunResult` / `SourceResult` / `SinkResult` / `OrionConfResult`
- 是否把 `Result<T, String>` 暴露到了边界上

### 2.2 source 是否被保留

- 是否存在 `map_err(|e| e.to_string())`
- 是否存在 `format!("{}", e)` 之后再构造结构错误
- 是否存在 `SinkReason::Sink(e.to_string())`
- 是否存在 `SourceReason::Disconnect(e.to_string())`
- 是否存在本该 `with_source(e)` 却只保留 detail 的地方

### 2.3 detail 是否有效

- `detail` 是否说明了当前层正在做什么
- 是否只有“配置错误”“执行失败”“error”这类无效 detail
- 是否能帮助用户定位问题，而不仅仅是开发者猜测

### 2.4 定位信息是否充分

- 文件类错误是否补了 `with(path)`
- 配置类错误是否能定位到文件 / group / instance / 字段
- 是否补了 `want("operation")` 来说明当前操作

### 2.5 CLI 是否最终可见

- 如果这个错误会到 CLI，最终能否提取出：
  - reason
  - detail
  - file/location
  - parse excerpt
  - root cause

## 3. 推荐写法速查

### 配置 I/O

```rust,ignore
std::fs::read_to_string(&path)
    .owe_conf_source()
    .with(&path)
    .want("read config")?;
```

### 运行时 I/O

```rust,ignore
stream.write_all(bytes).await.map_err(|e| {
    SinkReason::sink("tcp send failed").err_source(e)
})?;
```

### source 读取

```rust,ignore
socket.recv_from(&mut buf).await.map_err(|e| {
    SourceReason::Disconnect("udp recv_from failed".to_string()).err_source(e)
})?;
```

### 应用层提升

```rust,ignore
result.to_run_err_source("load project config")?;
```

## 4. 典型反模式

以下写法在核心链路中通常应直接要求修改：

```rust,ignore
map_err(|e| anyhow::anyhow!("{}", e))
```

```rust,ignore
map_err(|e| SinkReason::Sink(format!("{}", e)).to_err())
```

```rust,ignore
map_err(|e| RunReason::from_conf().to_err().with_detail(e.to_string()))
```

```rust,ignore
let _ = result.ok();
```

## 5. 判定等级

### 必须修改

- 核心接口返回 `anyhow::Result`
- 关键边界丢失 source
- CLI 可见错误没有有效 detail
- 配置错误缺乏文件/路径定位

### 建议修改

- detail 过于笼统
- 同类逻辑未复用 `owe_conf_source().with(...).want(...)`
- `to_run_err(...)` 可提升为 `to_run_err_source(...)`

### 可接受

- 测试代码的 `anyhow::Result`
- 不跨边界的小型辅助逻辑
- 迁移过程中的短期过渡写法

## 6. 评审时可直接问的问题

评审者可以直接问：

1. 这个函数为什么不是结构化错误返回值？
2. 上游真实错误对象为什么没有保留？
3. 这个 detail 对用户排障是否足够？
4. 这里是否应该补 `with(path)` 或 `want(...)`？
5. 这个错误最后到 CLI 后，用户能看到什么？

## 7. 默认结论

如果拿不准，默认按以下原则判断：

- 保留 source 比压平成字符串更正确
- 结构化错误比 `anyhow::Result` 更适合核心链路
- 可以被 CLI / 日志 / 监控利用的信息，应该尽量在边界保留下来
