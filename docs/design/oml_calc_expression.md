# OML 算术表达式设计

## 背景

当前 OML 已支持：
- `match` 条件匹配
- `object` 结构构造
- `pipe` 单值变换
- `lookup_nocase(...)` 静态字典查表

但仍缺少一套统一的算术表达式能力，导致以下场景需要拆成多步或无法直写：
- 风险分数、比例、差值、偏移量计算
- 字段与常量混合运算
- 基于绝对值/取整的派生字段

目标是在不破坏现有 DSL 结构的前提下，为 OML 增加一套明确、可预测、可测试的算术表达式语义。

## 目标

### 需求范围

- 支持常用数学表达式
- 支持字段与常量混合运算
- 支持 `+ - * / %`
- 支持 `abs / round / floor / ceil`
- 明确除零、缺失字段、非数值输入行为

### 非目标

首版不支持：
- 裸中缀表达式作为顶层赋值语法
- 字符串自动转数值
- 比较运算（`> < >= <= == !=`）
- 逻辑运算（`&& || !`）
- `pow / min / max / clamp / log / exp`
- 多返回值或向量运算

## DSL 方案

### 顶层入口

采用显式入口 `calc(...)`，而不是直接放开裸中缀表达式。

```oml
risk_score : float = calc(read(cpu) * 0.7 + read(mem) * 0.3);
delta      : digit = calc(read(cur) - read(prev));
ratio      : float = calc(read(ok_cnt) / read(total_cnt));
bucket     : digit = calc(read(uid) % 16);
distance   : float = calc(abs(read(actual) - read(expect)));
```

这样做的原因：
- 与现有 `match(...)` / `pipe ...` / `lookup_nocase(...)` 风格一致
- 避免 parser 顶层歧义扩散到整个 OML 语法
- 便于未来单独扩展数学表达式子语法

### 语法草案

```ebnf
calc_expr     = "calc", "(", expr, ")" ;

expr          = add_expr ;
add_expr      = mul_expr, { ("+" | "-"), mul_expr } ;
mul_expr      = unary_expr, { ("*" | "/" | "%"), unary_expr } ;
unary_expr    = [ "-" ], primary_expr ;
primary_expr  = number
              | read_expr
              | take_expr
              | fun_expr
              | "(", expr, ")" ;

fun_expr      = calc_fun, "(", expr, ")" ;
calc_fun      = "abs" | "round" | "floor" | "ceil" ;
```

### 示例

```oml
loss_ratio : float = calc(read(loss) / read(total));
offset     : digit = calc(read(cur) - 100);
slot       : digit = calc(read(id) % 64);
score      : digit = calc(round((read(err_cnt) * 100) / read(total_cnt)));
safe_dist  : float = calc(abs(read(actual) - read(expect)));
```

## 类型语义

### 输入类型

`calc(...)` 的操作数只接受数值：
- `digit`
- `float`
- 数值字面量

不接受：
- `chars`
- `bool`
- `time`
- `ip`
- `object`
- `array`

### 运算结果类型

#### `+ - *`

- `digit op digit -> digit`
- 只要任一操作数为 `float`，结果为 `float`

#### `/`

- 一律返回 `float`

示例：
- `10 / 2 -> 5.0`
- `9 / 2 -> 4.5`

#### `%`

- 仅支持 `digit % digit -> digit`
- 任一操作数为 `float` 视为非法

#### `abs`

- `abs(digit) -> digit`
- `abs(float) -> float`

#### `round / floor / ceil`

- 返回 `digit`

示例：
- `round(1.2) -> 1`
- `round(1.8) -> 2`
- `floor(1.8) -> 1`
- `ceil(1.2) -> 2`

## 异常与边界行为

首版统一采用“表达式失败即返回 `ignore`”策略，不 panic，不隐式兜底为 `0`。

### 1. 除零

```oml
ratio = calc(read(a) / read(b));
```

当除数为 `0` 时：
- 整个 `calc(...)` 结果为 `ignore`
- 记录诊断：`math_divide_by_zero`

### 2. 模零

```oml
slot = calc(read(id) % read(m));
```

当右侧为 `0` 时：
- 整个结果为 `ignore`
- 记录诊断：`math_mod_by_zero`

### 3. 缺失字段

```oml
delta = calc(read(cur) - read(prev));
```

任一字段缺失时：
- 整个结果为 `ignore`
- 记录诊断：`math_missing_operand`

### 4. 非数值输入

```oml
score = calc(read(status) + 1);
```

任一操作数非数值时：
- 整个结果为 `ignore`
- 记录诊断：`math_non_numeric_operand`

### 5. 非法 `%` 输入

```oml
x = calc(read(a) % 2.5);
```

当 `%` 操作数不是 `digit` 时：
- 整个结果为 `ignore`
- 记录诊断：`math_invalid_mod_operand`

### 6. 函数参数非法

```oml
x = calc(abs(read(name)));
```

当函数参数不是数值时：
- 整个结果为 `ignore`
- 记录诊断：`math_invalid_argument`

## 设计决策

### 为什么失败返回 `ignore`

理由：
- 与现有 OML “无有效值则跳过产出”的风格一致
- 便于和默认值机制组合
- 避免把业务异常伪装成合法数值 `0`

示例：

```oml
raw_ratio  = calc(read(ok_cnt) / read(total_cnt));
safe_ratio = read(raw_ratio) { _ : float(0.0) };
```

### 为什么不做字符串自动转数值

不建议首版做 `"123"` -> `123` 的隐式转换，原因：
- 行为不稳定，`"001"`、`"1.2"`、`"1e3"`、`"NaN"` 都会引入歧义
- 与现有 typed literal / typed field 体系不一致
- 会掩盖上游数据建模问题

如确有需要，后续可以单独增加显式转换函数：
- `to_digit(...)`
- `to_float(...)`

### 为什么不用 pipe

`pipe` 更适合单输入单输出的线性变换。  
算术表达式本质是：
- 多输入
- 有运算优先级
- 有中间表达式树

因此应作为独立 operation，而不是塞进 `ValueProcessor`。

## 实现方案

### AST

新增 `CalcOperation` 与表达式树：

```rust
pub enum CalcExpr {
    Const(CalcNumber),
    Accessor(DirectAccessor),
    UnaryNeg(Box<CalcExpr>),
    Binary {
        op: CalcOp,
        lhs: Box<CalcExpr>,
        rhs: Box<CalcExpr>,
    },
    Func {
        fun: CalcFun,
        arg: Box<CalcExpr>,
    },
}

pub enum CalcOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

pub enum CalcFun {
    Abs,
    Round,
    Floor,
    Ceil,
}
```

### 挂载位置

- `PreciseEvaluator::Calc(CalcOperation)`
- parser 新增 `calc_prm.rs`
- evaluator 新增 `extract/operations/calc.rs`

### 执行流程

1. 递归求值 `CalcExpr`
2. 将中间结果统一表示为内部数值类型：
   ```rust
   enum CalcNumber {
       Digit(i64),
       Float(f64),
   }
   ```
3. 根据运算规则计算结果
4. 产出 `DataField`
5. 任何非法情况记录诊断并产出 `ignore`

### 诊断建议

在 evaluator 中增加结构化诊断：
- `math_divide_by_zero`
- `math_mod_by_zero`
- `math_missing_operand`
- `math_non_numeric_operand`
- `math_invalid_mod_operand`
- `math_invalid_argument`

## 测试矩阵

### 解析测试

- `calc(1 + 2)`
- `calc(read(a) + 1)`
- `calc((read(a) + 1) * 2)`
- `calc(abs(read(x) - 1))`
- `calc(round(read(x) / 3))`

### 类型测试

- `digit + digit -> digit`
- `digit + float -> float`
- `digit / digit -> float`
- `digit % digit -> digit`
- `float % digit -> ignore`

### 异常测试

- 除零 -> `ignore`
- `% 0` -> `ignore`
- 缺失字段 -> `ignore`
- `chars + digit` -> `ignore`
- `abs(chars)` -> `ignore`

### 优先级测试

- `1 + 2 * 3 = 7`
- `(1 + 2) * 3 = 9`
- `-read(x) * 2`

### 函数测试

- `abs(-3) -> 3`
- `round(1.6) -> 2`
- `floor(1.6) -> 1`
- `ceil(1.2) -> 2`

## 示例

### 风险分数

```oml
risk_score : float = calc(read(cpu) * 0.7 + read(mem) * 0.3);
```

### 错误率

```oml
error_ratio : float = calc(read(err_cnt) / read(total_cnt));
```

### 哈希分桶

```oml
bucket : digit = calc(read(uid) % 16);
```

### 偏差绝对值

```oml
distance : float = calc(abs(read(actual) - read(expect)));
```

### 取整百分比

```oml
error_pct : digit = calc(round((read(err_cnt) * 100) / read(total_cnt)));
```

## 结论

推荐按以下顺序落地：

1. `calc(...)` 顶层入口
2. `+ - * / %` 与括号优先级
3. `abs / round / floor / ceil`
4. 统一失败语义为 `ignore`
5. 结构化诊断与文档补齐

这套方案兼顾了：
- 语法清晰
- parser 改动可控
- 运行时行为稳定
- 后续可演进空间明确
