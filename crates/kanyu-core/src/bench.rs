//! 性能基准场景生成器（ARCHITECTURE §8 性能目标 / §9.1「先建基准场景再谈优化」）。
//!
//! 确定性伪随机（xorshift64*，零依赖）：同种子同输出（单测断言），基准可复现。
//! 场景：
//! - [`mixed`]：点/线/面混合数据集（规模参数化，各约 1/3；属性含 id/v 数值字段）；
//! - [`overlay_pair`]：面叠加对（两图层各 N 个部分重叠方格，B 层错半格保证重叠）。
//!
//! 生成器为纯函数；大文件写出由 CLI 侧落 `target/bench/`（不入仓库）。
//! 坐标域取中国范围（经度 73–135，纬度 18–53），单位为度。

use geojson::{Feature, FeatureCollection, Geometry, Value};

/// 坐标域（minx, miny, maxx, maxy）：中国范围。
const DOMAIN: (f64, f64, f64, f64) = (73.0, 18.0, 135.0, 53.0);

/// xorshift64* 伪随机数发生器（确定性、零依赖）。
#[derive(Debug, Clone)]
pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// 以种子构造（0 归一为黄金比例常数，避免零态锁死）。
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 下一伪随机数（xorshift64*）。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// [0, 1) 均匀浮点。
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// [lo, hi) 均匀浮点。
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

/// 空集合。
fn empty_collection() -> FeatureCollection {
    FeatureCollection {
        bbox: None,
        features: Vec::new(),
        foreign_members: None,
    }
}

/// 构造要素（属性 {"id": 序号, "v": [0,100) 数值}——供统计/连接类基准消费）。
fn feature(id: usize, v: f64, value: Value) -> Feature {
    let mut props = serde_json::Map::new();
    props.insert("id".to_string(), serde_json::Value::from(id));
    props.insert("v".to_string(), serde_json::Value::from(v));
    Feature {
        bbox: None,
        geometry: Some(Geometry::new(value)),
        id: None,
        properties: Some(props),
        foreign_members: None,
    }
}

/// 域内随机点。
fn random_point(rng: &mut Xorshift64) -> Vec<f64> {
    let (x0, y0, x1, y1) = DOMAIN;
    vec![rng.range(x0, x1), rng.range(y0, y1)]
}

/// 点/线/面混合数据集：各约 1/3（点线各 size/3，其余为面）。
/// 线为 5 顶点随机游走折线；面为边长约 0.05° 的方格。
pub fn mixed(size: usize, seed: u64) -> FeatureCollection {
    let mut rng = Xorshift64::new(seed);
    let mut out = empty_collection();
    let n_lines = size / 3;
    let n_points = size / 3;
    for i in 0..size {
        let v = rng.range(0.0, 100.0);
        let value = if i < n_points {
            Value::Point(random_point(&mut rng))
        } else if i < n_points + n_lines {
            // 5 顶点随机游走折线（步长 ±0.1°，域内钳制）。
            let (x0, y0, x1, y1) = DOMAIN;
            let mut pt = random_point(&mut rng);
            let mut coords = Vec::with_capacity(5);
            coords.push(pt.clone());
            for _ in 0..4 {
                pt[0] = (pt[0] + rng.range(-0.1, 0.1)).clamp(x0, x1);
                pt[1] = (pt[1] + rng.range(-0.1, 0.1)).clamp(y0, y1);
                coords.push(pt.clone());
            }
            Value::LineString(coords)
        } else {
            let c = random_point(&mut rng);
            Value::Polygon(vec![square_at(c[0], c[1], 0.05)])
        };
        out.features.push(feature(i, v, value));
    }
    out
}

/// (x, y) 为左下角、边长 side 的方格环。
fn square_at(x: f64, y: f64, side: f64) -> Vec<Vec<f64>> {
    vec![
        vec![x, y],
        vec![x + side, y],
        vec![x + side, y + side],
        vec![x, y + side],
        vec![x, y],
    ]
}

/// 面叠加场景：两图层各 N 个方格。A 层按 ⌈√N⌉ 网格铺满坐标域
/// （边长 = 0.8×格距）；B 层错半格（重叠保证：每个 B 格与 4 个 A 格相交，
/// 边缘格除外）。两侧要素序一致（逐行扫描），便于复现。
pub fn overlay_pair(n: usize, seed: u64) -> (FeatureCollection, FeatureCollection) {
    let mut rng = Xorshift64::new(seed);
    let (x0, y0, x1, y1) = DOMAIN;
    let g = (n as f64).sqrt().ceil() as usize;
    let (sx, sy) = ((x1 - x0) / g as f64, (y1 - y0) / g as f64);
    let (mut a, mut b) = (empty_collection(), empty_collection());
    for i in 0..n {
        let (row, col) = (i / g, i % g);
        let (ax, ay) = (x0 + col as f64 * sx, y0 + row as f64 * sy);
        a.features.push(feature(
            i,
            rng.range(0.0, 100.0),
            Value::Polygon(vec![square_at(ax, ay, sx * 0.8)]),
        ));
        // B 层错半格（不越界）。
        let bx = (ax + sx * 0.5).min(x1 - sx * 0.8);
        let by = (ay + sy * 0.5).min(y1 - sy * 0.8);
        b.features.push(feature(
            i,
            rng.range(0.0, 100.0),
            Value::Polygon(vec![square_at(bx, by, sx * 0.8)]),
        ));
    }
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift_is_deterministic_and_seed_dependent() {
        let mut a = Xorshift64::new(42);
        let mut b = Xorshift64::new(42);
        let mut c = Xorshift64::new(43);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64(), "同种子同序列");
        }
        let sa: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let sc: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_ne!(sa, sc, "异种子异序列");
        // 零种子归一（不锁死零态）。
        let mut z = Xorshift64::new(0);
        assert!(z.next_u64() != 0);
    }

    #[test]
    fn mixed_is_deterministic_and_right_size() {
        let a = mixed(1000, 42);
        let b = mixed(1000, 42);
        assert_eq!(a, b, "同种子同输出");
        assert_eq!(a.features.len(), 1000);
        let c = mixed(1000, 7);
        assert_ne!(a, c, "异种子异输出");
        // 类型构成：点线各 1/3，其余为面；属性含 id/v。
        let (mut p, mut l, mut g) = (0, 0, 0);
        for f in &a.features {
            match &f.geometry.as_ref().unwrap().value {
                Value::Point(_) => p += 1,
                Value::LineString(_) => l += 1,
                Value::Polygon(_) => g += 1,
                other => panic!("未知几何: {other:?}"),
            }
            let props = f.properties.as_ref().unwrap();
            assert!(props["id"].is_number() && props["v"].is_number());
        }
        assert_eq!((p, l, g), (333, 333, 334));
        // 坐标落在坐标域内。
        if let Value::Point(pt) = &a.features[0].geometry.as_ref().unwrap().value {
            assert!(pt[0] >= 73.0 && pt[0] < 135.0 && pt[1] >= 18.0 && pt[1] < 53.0);
        }
    }

    #[test]
    fn overlay_pair_right_size_and_guaranteed_overlap() {
        use geo::Intersects;
        let (a, b) = overlay_pair(64, 1);
        assert_eq!(a.features.len(), 64);
        assert_eq!(b.features.len(), 64);
        // 确定性。
        assert_eq!(a, overlay_pair(64, 1).0);
        // 中部 B 格与 A 格相交（第 10 格非边缘）。
        let ga =
            geo_types::Geometry::<f64>::try_from(&a.features[9].geometry.as_ref().unwrap().value)
                .unwrap();
        let gb =
            geo_types::Geometry::<f64>::try_from(&b.features[9].geometry.as_ref().unwrap().value)
                .unwrap();
        assert!(ga.intersects(&gb), "错半格布设应保证重叠");
    }
}
