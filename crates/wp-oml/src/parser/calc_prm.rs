use crate::language::{CalcExpr, CalcFun, CalcNumber, CalcOp, CalcOperation, PreciseEvaluator};
use crate::parser::oml_aggregate::oml_var_get;
use winnow::ascii::{digit1, multispace0};
use winnow::error::{ContextError, ErrMode, StrContext, StrContextValue};
use winnow::stream::Stream;
use wp_parser::Parser;
use wp_parser::WResult;
use wp_parser::atom::take_var_name;
use wp_parser::utils::get_scope;

pub fn oml_aga_calc(data: &mut &str) -> WResult<PreciseEvaluator> {
    let op = oml_calc.parse_next(data)?;
    Ok(PreciseEvaluator::Calc(op))
}

pub fn oml_calc(data: &mut &str) -> WResult<CalcOperation> {
    multispace0.parse_next(data)?;
    "calc"
        .context(StrContext::Label("oml keyword"))
        .context(StrContext::Expected(StrContextValue::Description(
            "need 'calc' keyword",
        )))
        .parse_next(data)?;

    let scope = get_scope(data, '(', ')')?;
    let mut expr_data = scope;
    let expr = parse_expr(&mut expr_data)?;
    multispace0.parse_next(&mut expr_data)?;
    if !expr_data.is_empty() {
        return Err(ErrMode::Backtrack(ContextError::new()));
    }
    Ok(CalcOperation::new(expr))
}

fn consume_symbol(data: &mut &str, symbol: char) -> bool {
    let cp = data.checkpoint();
    *data = data.trim_start();
    if data.starts_with(symbol) {
        *data = &data[symbol.len_utf8()..];
        true
    } else {
        data.reset(&cp);
        false
    }
}

fn parse_expr(data: &mut &str) -> WResult<CalcExpr> {
    parse_add(data)
}

fn parse_add(data: &mut &str) -> WResult<CalcExpr> {
    let mut expr = parse_mul(data)?;
    loop {
        let op = if consume_symbol(data, '+') {
            Some(CalcOp::Add)
        } else if consume_symbol(data, '-') {
            Some(CalcOp::Sub)
        } else {
            None
        };
        let Some(op) = op else {
            break;
        };
        let rhs = parse_mul(data)?;
        expr = CalcExpr::Binary {
            op,
            lhs: Box::new(expr),
            rhs: Box::new(rhs),
        };
    }
    Ok(expr)
}

fn parse_mul(data: &mut &str) -> WResult<CalcExpr> {
    let mut expr = parse_unary(data)?;
    loop {
        let op = if consume_symbol(data, '*') {
            Some(CalcOp::Mul)
        } else if consume_symbol(data, '/') {
            Some(CalcOp::Div)
        } else if consume_symbol(data, '%') {
            Some(CalcOp::Mod)
        } else {
            None
        };
        let Some(op) = op else {
            break;
        };
        let rhs = parse_unary(data)?;
        expr = CalcExpr::Binary {
            op,
            lhs: Box::new(expr),
            rhs: Box::new(rhs),
        };
    }
    Ok(expr)
}

fn parse_unary(data: &mut &str) -> WResult<CalcExpr> {
    if consume_symbol(data, '-') {
        return Ok(CalcExpr::UnaryNeg(Box::new(parse_unary(data)?)));
    }
    parse_primary(data)
}

fn parse_primary(data: &mut &str) -> WResult<CalcExpr> {
    multispace0.parse_next(data)?;

    let cp = data.checkpoint();
    if data.starts_with('(') {
        let scope = get_scope(data, '(', ')')?;
        let mut inner = scope;
        let expr = parse_expr(&mut inner)?;
        multispace0.parse_next(&mut inner)?;
        if !inner.is_empty() {
            return Err(ErrMode::Backtrack(ContextError::new()));
        }
        return Ok(expr);
    }
    data.reset(&cp);

    let cp = data.checkpoint();
    if let Ok(fun_expr) = parse_calc_fun(data) {
        return Ok(fun_expr);
    }
    data.reset(&cp);

    let cp = data.checkpoint();
    if let Ok(accessor) = oml_var_get.parse_next(data) {
        return Ok(CalcExpr::Accessor(accessor));
    }
    data.reset(&cp);

    let num = parse_number(data)?;
    Ok(CalcExpr::Const(num))
}

fn parse_calc_fun(data: &mut &str) -> WResult<CalcExpr> {
    multispace0.parse_next(data)?;
    let fun_name = take_var_name.parse_next(data)?;
    let fun = match fun_name {
        "abs" => CalcFun::Abs,
        "round" => CalcFun::Round,
        "floor" => CalcFun::Floor,
        "ceil" => CalcFun::Ceil,
        _ => return Err(ErrMode::Backtrack(ContextError::new())),
    };

    let scope = get_scope(data, '(', ')')?;
    let mut arg_data = scope;
    let arg = parse_expr(&mut arg_data)?;
    multispace0.parse_next(&mut arg_data)?;
    if !arg_data.is_empty() {
        return Err(ErrMode::Backtrack(ContextError::new()));
    }
    Ok(CalcExpr::Func {
        fun,
        arg: Box::new(arg),
    })
}

fn parse_number(data: &mut &str) -> WResult<CalcNumber> {
    let cp = data.checkpoint();
    *data = data.trim_start();
    let int_part = digit1.parse_next(data)?;
    let cp_dot = data.checkpoint();
    if data.starts_with('.') {
        *data = &data[1..];
        let frac_part = digit1.parse_next(data)?;
        let raw = format!("{}.{}", int_part, frac_part);
        let value = raw
            .parse::<f64>()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        return Ok(CalcNumber::Float(value));
    }
    data.reset(&cp_dot);
    let value = int_part
        .parse::<i64>()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    if data.starts_with('.') {
        data.reset(&cp);
        return Err(ErrMode::Backtrack(ContextError::new()));
    }
    Ok(CalcNumber::Digit(value))
}

#[cfg(test)]
mod tests {
    use crate::parser::calc_prm::oml_calc;
    use crate::parser::utils::for_test::assert_oml_parse_ext;

    #[test]
    fn test_calc_parse() {
        let mut code = r#"calc(read(cpu) * 0.7 + read(mem) * 0.3)"#;
        let expect = r#"calc(((read(cpu) * 0.7) + (read(mem) * 0.3)))"#;
        assert_oml_parse_ext(&mut code, oml_calc, expect);
    }

    #[test]
    fn test_calc_parse_with_fun_and_paren() {
        let mut code = r#"calc(round((read(err_cnt) * 100) / read(total_cnt)))"#;
        let expect = r#"calc(round(((read(err_cnt) * 100) / read(total_cnt))))"#;
        assert_oml_parse_ext(&mut code, oml_calc, expect);
    }
}
