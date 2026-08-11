//! 属性字段计算（字段计算器核心）：递归下降表达式引擎 + 字段增删改名。
//!
//! ## 表达式语法
//!
//! - 字面量：数值（`1`、`-2.5`）、字符串（`'单引号'` 或 `"双引号"`， doubled quote
//!   转义）、布尔（`true`/`false`）、空（`null`）；
//! - 字段引用：裸标识符（含中文）或 `[字段名]` 形式；
//! - 几何虚列：`$area`（测地面积 ㎡，Karney 2013）、`$length`（测地长度 m）、
//!   `$x`/`$y`（点坐标或质心，与 geoprocess 代表点同口径）；
//! - 运算：`+ - * / %`（数值；`+` 对字符串为拼接）、比较 `= != < <= > >=`
//!   （数值/字符串）、逻辑 `and or not`、括号；
//! - 函数：`abs / round(x[,n]) / floor / ceil / sqrt / power(x,n) / min / max /
//!   upper / lower / length / trim / concat(多参) / coalesce(多参)`。
//!
//! 错误均为中文结构化（表达式解析错误带位置，求值错误由调用方补要素序号）。

use geojson::{FeatureCollection, Value as GeoValue};
use serde_json::{Map, Value as Json};

use crate::error::{KanyuError, Result};

// ===== 值与词法 =====

/// 表达式值。
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Null,
    Num(f64),
    Str(String),
    Bool(bool),
}

impl Value {
    /// 转 JSON（写回属性）。
    fn to_json(&self) -> Json {
        match self {
            Value::Null => Json::Null,
            Value::Num(n) => serde_json::Number::from_f64(*n)
                .map(Json::from)
                .unwrap_or(Json::Null),
            Value::Str(s) => Json::from(s.clone()),
            Value::Bool(b) => Json::from(*b),
        }
    }
    /// 字符串化（concat/比较提示用）。
    fn display(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Num(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
        }
    }
}

/// 词法单元。
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    /// [字段名] / $area 等几何虚列。
    Dollar(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    End,
}

/// 切词（关键字 and/or/not/true/false/null 大小写不敏感，归入 Ident 由解析器判别）。
fn lex(input: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '0'..='9' | '.' => {
                // 数值：整数/小数/科学计数（1.5e-3）。
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                    i += 1;
                    if i < chars.len() && matches!(chars[i], '+' | '-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let n: f64 = text
                    .parse()
                    .map_err(|_| KanyuError::InvalidQuery(format!("数值字面量非法: '{text}'")))?;
                out.push(Tok::Num(n));
            }
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                loop {
                    match chars.get(i) {
                        None => return Err(KanyuError::InvalidQuery("字符串未闭合".to_string())),
                        Some(&q) if q == quote => {
                            if chars.get(i + 1) == Some(&quote) {
                                s.push(quote); // doubled quote 转义
                                i += 2;
                            } else {
                                i += 1;
                                break;
                            }
                        }
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            '[' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(KanyuError::InvalidQuery("字段引用 […] 未闭合".to_string()));
                }
                out.push(Tok::Ident(chars[start..j].iter().collect()));
                i = j + 1;
            }
            '$' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j].is_alphanumeric() {
                    j += 1;
                }
                out.push(Tok::Dollar(chars[start..j].iter().collect()));
                i = j;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '+' => {
                out.push(Tok::Op("+"));
                i += 1;
            }
            '-' => {
                out.push(Tok::Op("-"));
                i += 1;
            }
            '*' => {
                out.push(Tok::Op("*"));
                i += 1;
            }
            '/' => {
                out.push(Tok::Op("/"));
                i += 1;
            }
            '%' => {
                out.push(Tok::Op("%"));
                i += 1;
            }
            '=' | '!' | '<' | '>' => {
                let two = chars.get(i + 1).copied();
                let op = match (c, two) {
                    ('=', Some('=')) => "==",
                    ('=', _) => "=",
                    ('!', Some('=')) => "!=",
                    ('<', Some('=')) => "<=",
                    ('>', Some('=')) => ">=",
                    ('<', _) => "<",
                    ('>', _) => ">",
                    _ => {
                        return Err(KanyuError::InvalidQuery(format!(
                            "无法识别的运算符: '{c}'（第 {} 字符）",
                            i + 1
                        )))
                    }
                };
                out.push(Tok::Op(op));
                i += if op.len() == 2 { 2 } else { 1 };
            }
            _ if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            _ => {
                return Err(KanyuError::InvalidQuery(format!(
                    "无法识别的字符: '{c}'（第 {} 字符）",
                    i + 1
                )))
            }
        }
    }
    out.push(Tok::End);
    Ok(out)
}

// ===== 语法（递归下降）=====

/// 表达式 AST。
#[derive(Debug)]
enum Expr {
    Lit(Value),
    Field(String),
    /// 几何虚列：area/length/x/y。
    Geom(String),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Box<Expr>, &'static str, Box<Expr>),
    Call(String, Vec<Expr>),
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }
    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }
    fn kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s.eq_ignore_ascii_case(kw))
    }

    /// or := and (or and)*
    fn parse_or(&mut self) -> Result<Box<Expr>> {
        let mut lhs = self.parse_and()?;
        while self.kw("or") {
            self.next();
            let rhs = self.parse_and()?;
            lhs = Box::new(Expr::Bin(lhs, "or", rhs));
        }
        Ok(lhs)
    }
    fn parse_and(&mut self) -> Result<Box<Expr>> {
        let mut lhs = self.parse_not()?;
        while self.kw("and") {
            self.next();
            let rhs = self.parse_not()?;
            lhs = Box::new(Expr::Bin(lhs, "and", rhs));
        }
        Ok(lhs)
    }
    fn parse_not(&mut self) -> Result<Box<Expr>> {
        if self.kw("not") {
            self.next();
            return Ok(Box::new(Expr::Not(self.parse_not()?)));
        }
        self.parse_cmp()
    }
    fn parse_cmp(&mut self) -> Result<Box<Expr>> {
        let lhs = self.parse_add()?;
        if let Tok::Op(op @ ("=" | "==" | "!=" | "<" | "<=" | ">" | ">=")) = self.peek() {
            let op = *op;
            self.next();
            let rhs = self.parse_add()?;
            return Ok(Box::new(Expr::Bin(lhs, op, rhs)));
        }
        Ok(lhs)
    }
    fn parse_add(&mut self) -> Result<Box<Expr>> {
        let mut lhs = self.parse_mul()?;
        while let Tok::Op(op @ ("+" | "-")) = self.peek() {
            let op = *op;
            self.next();
            let rhs = self.parse_mul()?;
            lhs = Box::new(Expr::Bin(lhs, op, rhs));
        }
        Ok(lhs)
    }
    fn parse_mul(&mut self) -> Result<Box<Expr>> {
        let mut lhs = self.parse_unary()?;
        while let Tok::Op(op @ ("*" | "/" | "%")) = self.peek() {
            let op = *op;
            self.next();
            let rhs = self.parse_unary()?;
            lhs = Box::new(Expr::Bin(lhs, op, rhs));
        }
        Ok(lhs)
    }
    fn parse_unary(&mut self) -> Result<Box<Expr>> {
        if self.peek() == &Tok::Op("-") {
            self.next();
            return Ok(Box::new(Expr::Neg(self.parse_unary()?)));
        }
        self.parse_primary()
    }
    fn parse_primary(&mut self) -> Result<Box<Expr>> {
        match self.next() {
            Tok::Num(n) => Ok(Box::new(Expr::Lit(Value::Num(n)))),
            Tok::Str(s) => Ok(Box::new(Expr::Lit(Value::Str(s)))),
            Tok::Dollar(name) => {
                let name = name.to_lowercase();
                if !matches!(name.as_str(), "area" | "length" | "x" | "y") {
                    return Err(err(format!(
                        "未知几何虚列: ${name}（支持 $area/$length/$x/$y）"
                    )));
                }
                Ok(Box::new(Expr::Geom(name)))
            }
            Tok::Ident(name) => {
                if name.eq_ignore_ascii_case("true") {
                    return Ok(Box::new(Expr::Lit(Value::Bool(true))));
                }
                if name.eq_ignore_ascii_case("false") {
                    return Ok(Box::new(Expr::Lit(Value::Bool(false))));
                }
                if name.eq_ignore_ascii_case("null") {
                    return Ok(Box::new(Expr::Lit(Value::Null)));
                }
                // 函数调用？
                if self.peek() == &Tok::LParen {
                    self.next();
                    let mut args = Vec::new();
                    if self.peek() != &Tok::RParen {
                        loop {
                            args.push(*self.parse_or()?);
                            if self.peek() == &Tok::Comma {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if self.next() != Tok::RParen {
                        return Err(KanyuError::InvalidQuery(format!(
                            "函数 {name} 调用缺少右括号"
                        )));
                    }
                    return Ok(Box::new(Expr::Call(name.to_lowercase(), args)));
                }
                Ok(Box::new(Expr::Field(name)))
            }
            Tok::LParen => {
                let e = self.parse_or()?;
                if self.next() != Tok::RParen {
                    return Err(KanyuError::InvalidQuery("括号未闭合".to_string()));
                }
                Ok(e)
            }
            other => Err(KanyuError::InvalidQuery(format!("此处不应出现: {other:?}"))),
        }
    }
}

/// 解析表达式为 AST。
fn parse(input: &str) -> Result<Box<Expr>> {
    let toks = lex(input)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.parse_or()?;
    if p.peek() != &Tok::End {
        return Err(KanyuError::InvalidQuery(format!(
            "表达式尾部有多余内容: {:?}",
            p.peek()
        )));
    }
    Ok(e)
}

// ===== 求值 =====

/// 求值上下文（单要素）。
struct Ctx<'a> {
    props: Option<&'a Map<String, Json>>,
    geom: Option<&'a GeoValue>,
}

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::InvalidQuery(msg.into())
}

fn need_num(v: &Value, what: &str) -> Result<f64> {
    match v {
        Value::Num(n) => Ok(*n),
        other => Err(err(format!("{what}须为数值，实为 {}", kind(other)))),
    }
}

fn need_str(v: &Value, what: &str) -> Result<String> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(err(format!("{what}须为字符串，实为 {}", kind(other)))),
    }
}

fn need_bool(v: &Value, what: &str) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(err(format!("{what}须为布尔，实为 {}", kind(other)))),
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "空值",
        Value::Num(_) => "数值",
        Value::Str(_) => "字符串",
        Value::Bool(_) => "布尔",
    }
}

fn eval(e: &Expr, ctx: &Ctx) -> Result<Value> {
    match e {
        Expr::Lit(v) => Ok(v.clone()),
        Expr::Field(name) => Ok(ctx
            .props
            .and_then(|p| p.get(name))
            .map(json_to_value)
            .unwrap_or(Value::Null)),
        Expr::Geom(name) => eval_geom(name, ctx),
        Expr::Neg(inner) => Ok(Value::Num(-need_num(&eval(inner, ctx)?, "取负")?)),
        Expr::Not(inner) => Ok(Value::Bool(!need_bool(&eval(inner, ctx)?, "not")?)),
        Expr::Bin(l, op, r) => eval_bin(l, op, r, ctx),
        Expr::Call(name, args) => eval_call(name, args, ctx),
    }
}

fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Num(n.as_f64().unwrap_or(f64::NAN)),
        Json::String(s) => Value::Str(s.clone()),
        // 数组/对象不参与计算，字符串化处理。
        other => Value::Str(other.to_string()),
    }
}

fn eval_bin(l: &Expr, op: &str, r: &Expr, ctx: &Ctx) -> Result<Value> {
    // 逻辑运算。
    if op == "and" || op == "or" {
        let a = need_bool(&eval(l, ctx)?, op)?;
        // 短路。
        let b = if op == "and" && !a {
            false
        } else if op == "or" && a {
            true
        } else {
            need_bool(&eval(r, ctx)?, op)?
        };
        return Ok(Value::Bool(b));
    }
    let a = eval(l, ctx)?;
    let b = eval(r, ctx)?;
    // NULL 传播（QGIS/SQL 语义）：算术与大小比较遇空值 → 空值；
    // 等值比较例外（下方单独处理）。
    if matches!(op, "+" | "-" | "*" | "/" | "%" | "<" | "<=" | ">" | ">=")
        && (a == Value::Null || b == Value::Null)
    {
        return Ok(Value::Null);
    }
    match op {
        "+" => match (&a, &b) {
            (Value::Num(x), Value::Num(y)) => Ok(Value::Num(x + y)),
            (Value::Str(x), Value::Str(y)) => Ok(Value::Str(format!("{x}{y}"))),
            _ => Err(err(format!(
                "「+」两侧类型须同为数值或同为字符串（实得 {} 与 {}）",
                kind(&a),
                kind(&b)
            ))),
        },
        "-" | "*" | "/" | "%" => {
            let (x, y) = (need_num(&a, op)?, need_num(&b, op)?);
            if (op == "/" || op == "%") && y == 0.0 {
                return Err(err("除数为 0"));
            }
            Ok(Value::Num(match op {
                "-" => x - y,
                "*" => x * y,
                "/" => x / y,
                _ => x % y,
            }))
        }
        "=" | "!=" => {
            let eq = match (&a, &b) {
                (Value::Null, Value::Null) => true,
                (Value::Null, _) | (_, Value::Null) => false,
                (Value::Num(x), Value::Num(y)) => x == y,
                (Value::Str(x), Value::Str(y)) => x == y,
                (Value::Bool(x), Value::Bool(y)) => x == y,
                _ => {
                    return Err(err(format!(
                        "比较两侧类型不一致（{} 与 {}）",
                        kind(&a),
                        kind(&b)
                    )))
                }
            };
            Ok(Value::Bool(if op == "=" { eq } else { !eq }))
        }
        "<" | "<=" | ">" | ">=" => {
            let ord = match (&a, &b) {
                (Value::Num(x), Value::Num(y)) => x.partial_cmp(y),
                (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
                _ => {
                    return Err(err(format!(
                        "比较两侧类型须同为数值或同为字符串（实得 {} 与 {}）",
                        kind(&a),
                        kind(&b)
                    )))
                }
            };
            let Some(o) = ord else {
                return Err(err("存在 NaN，无法比较"));
            };
            let lt = o == std::cmp::Ordering::Less;
            let eq = o == std::cmp::Ordering::Equal;
            Ok(Value::Bool(match op {
                "<" => lt,
                "<=" => lt || eq,
                ">" => !lt && !eq,
                _ => !lt,
            }))
        }
        _ => Err(err(format!("未知运算符 {op}"))),
    }
}

fn eval_call(name: &str, args: &[Expr], ctx: &Ctx) -> Result<Value> {
    let vals: Vec<Value> = args.iter().map(|a| eval(a, ctx)).collect::<Result<_>>()?;
    let arity = |want: &[usize]| -> Result<()> {
        if want.contains(&vals.len()) {
            Ok(())
        } else {
            Err(err(format!(
                "函数 {name} 参数个数须为 {}（实得 {}）",
                want.iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                vals.len()
            )))
        }
    };
    match name {
        "abs" => {
            arity(&[1])?;
            Ok(Value::Num(need_num(&vals[0], "abs")?.abs()))
        }
        "floor" => {
            arity(&[1])?;
            Ok(Value::Num(need_num(&vals[0], "floor")?.floor()))
        }
        "ceil" => {
            arity(&[1])?;
            Ok(Value::Num(need_num(&vals[0], "ceil")?.ceil()))
        }
        "sqrt" => {
            arity(&[1])?;
            let x = need_num(&vals[0], "sqrt")?;
            if x < 0.0 {
                return Err(err("sqrt 不接受负数"));
            }
            Ok(Value::Num(x.sqrt()))
        }
        "round" => {
            arity(&[1, 2])?;
            let x = need_num(&vals[0], "round")?;
            let n = if vals.len() > 1 {
                need_num(&vals[1], "round 位数")? as i32
            } else {
                0
            };
            let k = 10f64.powi(n);
            Ok(Value::Num((x * k).round() / k))
        }
        "power" => {
            arity(&[2])?;
            Ok(Value::Num(
                need_num(&vals[0], "power 底数")?.powf(need_num(&vals[1], "power 指数")?),
            ))
        }
        "min" | "max" => {
            if vals.is_empty() {
                return Err(err(format!("函数 {name} 至少一个参数")));
            }
            let nums: Result<Vec<f64>> = vals.iter().map(|v| need_num(v, name)).collect();
            let nums = nums?;
            let f = if name == "min" { f64::min } else { f64::max };
            Ok(Value::Num(nums.into_iter().reduce(f).unwrap()))
        }
        "upper" => {
            arity(&[1])?;
            Ok(Value::Str(need_str(&vals[0], "upper")?.to_uppercase()))
        }
        "lower" => {
            arity(&[1])?;
            Ok(Value::Str(need_str(&vals[0], "lower")?.to_lowercase()))
        }
        "trim" => {
            arity(&[1])?;
            Ok(Value::Str(need_str(&vals[0], "trim")?.trim().to_string()))
        }
        "length" => {
            arity(&[1])?;
            Ok(Value::Num(
                need_str(&vals[0], "length")?.chars().count() as f64
            ))
        }
        "concat" => Ok(Value::Str(vals.iter().map(|v| v.display()).collect())),
        "coalesce" => Ok(vals
            .into_iter()
            .find(|v| *v != Value::Null)
            .unwrap_or(Value::Null)),
        _ => Err(err(format!("未知函数: {name}"))),
    }
}

/// 几何虚列求值（$area/$length/$x/$y；测地口径与 geoprocess 一致）。
fn eval_geom(name: &str, ctx: &Ctx) -> Result<Value> {
    use geo::algorithm::line_measures::{Geodesic, Length as _};
    let Some(g) = ctx
        .geom
        .and_then(|v| geo_types::Geometry::<f64>::try_from(v).ok())
    else {
        return Ok(Value::Null); // 无几何 → 空值
    };
    match name {
        "area" => {
            use geo::algorithm::GeodesicArea;
            let area = match &g {
                geo_types::Geometry::Polygon(p) => p.geodesic_area_unsigned(),
                geo_types::Geometry::MultiPolygon(m) => m.geodesic_area_unsigned(),
                _ => 0.0,
            };
            Ok(Value::Num(area))
        }
        "length" => {
            let len = match &g {
                geo_types::Geometry::LineString(l) => Geodesic.length(l),
                geo_types::Geometry::MultiLineString(m) => Geodesic.length(m),
                geo_types::Geometry::Polygon(p) => Geodesic.length(p.exterior()),
                geo_types::Geometry::MultiPolygon(m) => {
                    m.iter().map(|p| Geodesic.length(p.exterior())).sum()
                }
                _ => 0.0,
            };
            Ok(Value::Num(len))
        }
        "x" | "y" => {
            use geo::Centroid;
            let pt = match &g {
                geo_types::Geometry::Point(p) => Some(*p),
                other => other.centroid(),
            };
            match pt {
                Some(p) => Ok(Value::Num(if name == "x" { p.x() } else { p.y() })),
                None => Ok(Value::Null),
            }
        }
        other => Err(err(format!(
            "未知几何虚列: ${other}（支持 $area/$length/$x/$y）"
        ))),
    }
}

// ===== 公共 API =====

/// 字段计算器：逐要素求值表达式并写入 `target` 字段（不存在则新建，存在则覆盖）。
/// 解析/求值错误带要素序号（中文）。
pub fn calc_field(
    collection: &FeatureCollection,
    target: &str,
    expr: &str,
) -> Result<FeatureCollection> {
    let ast = parse(expr)?;
    let mut out = collection.clone();
    for (idx, feature) in out.features.iter_mut().enumerate() {
        // 先求值（不可变借用在本块内结束），再写字段。
        let value = {
            let ctx = Ctx {
                props: feature.properties.as_ref(),
                geom: feature.geometry.as_ref().map(|g| &g.value),
            };
            eval(&ast, &ctx).map_err(|e| err(format!("要素 #{idx}（{expr}）: {e}")))?
        };
        feature
            .properties
            .get_or_insert_with(Default::default)
            .insert(target.to_string(), value.to_json());
    }
    Ok(out)
}

/// 添加字段（全部要素写入默认值；字段已存在则报错）。
pub fn add_field(
    collection: &FeatureCollection,
    name: &str,
    default: Option<Json>,
) -> Result<FeatureCollection> {
    if name.trim().is_empty() {
        return Err(err("字段名不能为空"));
    }
    if collection
        .features
        .iter()
        .any(|f| f.properties.as_ref().is_some_and(|p| p.contains_key(name)))
    {
        return Err(err(format!("字段已存在: {name}")));
    }
    let mut out = collection.clone();
    for feature in &mut out.features {
        feature
            .properties
            .get_or_insert_with(Default::default)
            .insert(name.to_string(), default.clone().unwrap_or(Json::Null));
    }
    Ok(out)
}

/// 删除字段（不存在不报错——幂等）。
pub fn delete_field(collection: &FeatureCollection, name: &str) -> Result<FeatureCollection> {
    let mut out = collection.clone();
    for feature in &mut out.features {
        if let Some(p) = &mut feature.properties {
            p.remove(name);
        }
    }
    Ok(out)
}

/// 重命名字段（旧名不存在 → 报错；新名已存在 → 报错）。
pub fn rename_field(
    collection: &FeatureCollection,
    old: &str,
    new: &str,
) -> Result<FeatureCollection> {
    if new.trim().is_empty() {
        return Err(err("新字段名不能为空"));
    }
    let exists_old = collection
        .features
        .iter()
        .any(|f| f.properties.as_ref().is_some_and(|p| p.contains_key(old)));
    if !exists_old {
        return Err(err(format!("字段不存在: {old}")));
    }
    if collection
        .features
        .iter()
        .any(|f| f.properties.as_ref().is_some_and(|p| p.contains_key(new)))
    {
        return Err(err(format!("字段已存在: {new}")));
    }
    let mut out = collection.clone();
    for feature in &mut out.features {
        if let Some(p) = &mut feature.properties {
            if let Some(v) = p.remove(old) {
                p.insert(new.to_string(), v);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::{feature::Id, Feature, Geometry};

    /// 构造单要素集合（属性 + 可选几何）。
    fn coll(props: &[(&str, Json)], geom: Option<Geometry>) -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: vec![Feature {
                bbox: None,
                geometry: geom,
                id: Some(Id::String("f0".into())),
                properties: Some(
                    props
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect(),
                ),
                foreign_members: None,
            }],
            foreign_members: None,
        }
    }

    fn point() -> Geometry {
        Geometry::new(GeoValue::Point(vec![116.4, 39.9]))
    }

    fn get(c: &FeatureCollection, field: &str) -> Json {
        c.features[0]
            .properties
            .as_ref()
            .unwrap()
            .get(field)
            .cloned()
            .unwrap()
    }

    fn calc(c: &FeatureCollection, expr: &str) -> Json {
        get(&calc_field(c, "t", expr).unwrap(), "t")
    }

    #[test]
    fn precedence_and_parens() {
        let c = coll(&[], None);
        assert_eq!(calc(&c, "1 + 2 * 3"), Json::from(7.0));
        assert_eq!(calc(&c, "(1 + 2) * 3"), Json::from(9.0));
        assert_eq!(calc(&c, "10 % 3"), Json::from(1.0));
        assert_eq!(calc(&c, "-2 + 5"), Json::from(3.0));
    }

    #[test]
    fn string_concat_and_funcs() {
        let c = coll(&[], None);
        assert_eq!(calc(&c, "'ab' + 'cd'"), Json::from("abcd"));
        assert_eq!(calc(&c, "upper('abc')"), Json::from("ABC"));
        assert_eq!(calc(&c, "concat('a', 1, 'b')"), Json::from("a1b"));
        assert_eq!(calc(&c, "length('汉字')"), Json::from(2.0));
        assert_eq!(calc(&c, "round(2.71828, 2)"), Json::from(2.72));
        assert_eq!(calc(&c, "power(2, 10)"), Json::from(1024.0));
        assert_eq!(calc(&c, "min(3, 1, 2)"), Json::from(1.0));
    }

    #[test]
    fn logic_and_compare() {
        let c = coll(&[("h", Json::from(50.0)), ("u", Json::from("办公"))], None);
        assert_eq!(calc(&c, "h > 40 and u = '办公'"), Json::from(true));
        assert_eq!(calc(&c, "not (h > 40) or h = 50"), Json::from(true));
        assert_eq!(calc(&c, "h != 50"), Json::from(false));
        assert_eq!(calc(&c, "'abc' < 'abd'"), Json::from(true));
    }

    #[test]
    fn bracket_field_and_cjk() {
        let c = coll(&[("建筑 高度", Json::from(88.0))], None);
        assert_eq!(calc(&c, "[建筑 高度] * 2"), Json::from(176.0));
        let c2 = coll(&[("层数", Json::from(6.0))], None);
        assert_eq!(calc(&c2, "层数 + 1"), Json::from(7.0));
    }

    #[test]
    fn coalesce_and_null() {
        let c = coll(&[("a", Json::Null)], None);
        assert_eq!(calc(&c, "coalesce(a, '缺省')"), Json::from("缺省"));
        assert_eq!(calc(&c, "missing = null"), Json::from(true));
        assert_eq!(calc(&c, "a = null"), Json::from(true));
    }

    #[test]
    fn geom_columns() {
        // 点：$x/$y。
        let c = coll(&[], Some(point()));
        assert_eq!(calc(&c, "$x"), Json::from(116.4));
        assert_eq!(calc(&c, "$y"), Json::from(39.9));
        // 面：$area > 0（1°×1° 方块在北京纬度约 9513 km² 量级，只验正数与有限）。
        let poly = Geometry::new(GeoValue::Polygon(vec![vec![
            vec![116.0, 39.0],
            vec![117.0, 39.0],
            vec![117.0, 40.0],
            vec![116.0, 40.0],
            vec![116.0, 39.0],
        ]]));
        let c2 = coll(&[], Some(poly));
        let area = calc(&c2, "$area").as_f64().unwrap();
        assert!(area > 8e9 && area < 1.2e10, "面积量级异常: {area}");
    }

    #[test]
    fn error_branches() {
        let c = coll(&[("h", Json::from(1.0))], None);
        // 解析错误。
        assert!(calc_field(&c, "t", "1 +").is_err());
        assert!(calc_field(&c, "t", "'未闭合").is_err());
        // 类型错误带要素序号。
        let e = calc_field(&c, "t", "h + 's'").unwrap_err().to_string();
        assert!(e.contains("要素 #0"), "{e}");
        assert!(calc_field(&c, "t", "1 / 0")
            .unwrap_err()
            .to_string()
            .contains("除数为 0"));
        assert!(calc_field(&c, "t", "nope_func(1)")
            .unwrap_err()
            .to_string()
            .contains("未知函数"));
        assert!(calc_field(&c, "t", "$volume")
            .unwrap_err()
            .to_string()
            .contains("未知几何虚列"));
    }

    #[test]
    fn field_crud() {
        let c = coll(&[("a", Json::from(1.0))], None);
        // 添加。
        let c2 = add_field(&c, "b", Some(Json::from("x"))).unwrap();
        assert_eq!(get(&c2, "b"), Json::from("x"));
        assert!(add_field(&c, "a", None).is_err()); // 已存在
        assert!(add_field(&c, "  ", None).is_err()); // 空名
                                                     // 重命名。
        let c3 = rename_field(&c2, "b", "c").unwrap();
        assert_eq!(get(&c3, "c"), Json::from("x"));
        assert!(rename_field(&c2, "ghost", "d").is_err());
        assert!(rename_field(&c2, "b", "a").is_err()); // 新名冲突
                                                       // 删除（幂等）。
        let c4 = delete_field(&c3, "c").unwrap();
        assert!(c4.features[0]
            .properties
            .as_ref()
            .unwrap()
            .get("c")
            .is_none());
        assert!(delete_field(&c4, "c").is_ok());
    }
}
