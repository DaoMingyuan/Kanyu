"""堪舆 Kanyu Python SDK —— Rust 内核的 Python 门面。

用法::

    import kanyu
    fc = kanyu.load("buildings.geojson")
    high = kanyu.query(fc, "height > 50")
    buf = kanyu.buffer(high, 500.0)
    kanyu.export(buf, "high_buffer.kdb", "kdb")

数据契约：GeoJSON FeatureCollection 文本（函数进出均为 JSON 字符串）。
渲染函数返回 PNG 字节 / SVG 文本。统计/度量/检索返回 JSON 文本
（json.loads 后使用）。run_tool / split_by_field 的多图层产出为
{名称: GeoJSON 对象} 的 JSON 文本（同样 json.loads 后使用）。
"""

import json as _json

from .kanyu import (  # noqa: F401 —— 原生函数再导出（Rust 内核实现）
    add_field,
    add_geometry_attributes,
    boundary,
    bounding_boxes,
    buffer,
    calc_field,
    centroid,
    concave_hull,
    convex_hull,
    count_points_in_polygon,
    create_grid,
    crs_info,
    delete_field,
    delete_holes,
    dissolve,
    distance_matrix,
    explode,
    export,
    extract_by_attribute,
    extract_by_location,
    field_stats,
    load,
    mean_coordinates,
    measure,
    merge,
    minimum_rotated_rect,
    multi_ring_buffer,
    nearest_neighbor,
    overlay,
    points_along_lines,
    query,
    rename_field,
    render_png,
    render_svg,
    reproject,
    run_tool,
    search_crs,
    simplify,
    sjoin,
    split_by_field,
    stats,
    toolbox_registry,
    topology,
    validate_crs,
    variable_buffer,
    version,
    zonal_stats,
)


class Layer:
    """图层便捷封装：持有一份 GeoJSON 文本，链式调用内核函数。

    >>> layer = kanyu.Layer.load("buildings.geojson")
    >>> layer.query("height > 50").buffer(500).export("out.fgb", "fgb")

    报告类方法（stats/field_stats/nearest_neighbor/distance_matrix/measure）
    返回解析后的 dict；split_by_field 返回 [(组值, Layer), …]。
    """

    def __init__(self, geojson_text: str):
        self.geojson = geojson_text

    @classmethod
    def load(cls, path: str) -> "Layer":
        from .kanyu import load as _load

        return cls(_load(path))

    # —— 选择 ——

    def query(self, expression: str) -> "Layer":
        from .kanyu import query as _query

        return Layer(_query(self.geojson, expression))

    def extract_by_attribute(self, expression: str) -> "Layer":
        from .kanyu import extract_by_attribute as _f

        return Layer(_f(self.geojson, expression))

    def extract_by_location(self, mask: "Layer", predicate: str) -> "Layer":
        from .kanyu import extract_by_location as _f

        return Layer(_f(self.geojson, mask.geojson, predicate))

    # —— 几何 ——

    def buffer(self, distance: float, segments: int = 16) -> "Layer":
        from .kanyu import buffer as _buffer

        return Layer(_buffer(self.geojson, distance, segments))

    def multi_ring_buffer(self, distances: list[float]) -> "Layer":
        from .kanyu import multi_ring_buffer as _f

        return Layer(_f(self.geojson, [float(d) for d in distances]))

    def variable_buffer(self, field: str, segments: int = 16) -> "Layer":
        from .kanyu import variable_buffer as _f

        return Layer(_f(self.geojson, field, segments))

    def simplify(self, tolerance: float) -> "Layer":
        from .kanyu import simplify as _simplify

        return Layer(_simplify(self.geojson, tolerance))

    def centroid(self) -> "Layer":
        from .kanyu import centroid as _f

        return Layer(_f(self.geojson))

    def convex_hull(self) -> "Layer":
        from .kanyu import convex_hull as _f

        return Layer(_f(self.geojson))

    def concave_hull(self, concavity: float = 2.0) -> "Layer":
        from .kanyu import concave_hull as _f

        return Layer(_f(self.geojson, concavity))

    def delete_holes(self, min_area: float | None = None) -> "Layer":
        from .kanyu import delete_holes as _f

        return Layer(_f(self.geojson, min_area))

    def explode(self) -> "Layer":
        from .kanyu import explode as _f

        return Layer(_f(self.geojson))

    def boundary(self) -> "Layer":
        from .kanyu import boundary as _f

        return Layer(_f(self.geojson))

    def bounding_boxes(self) -> "Layer":
        from .kanyu import bounding_boxes as _f

        return Layer(_f(self.geojson))

    def minimum_rotated_rect(self) -> "Layer":
        from .kanyu import minimum_rotated_rect as _f

        return Layer(_f(self.geojson))

    def points_along_lines(self, distance: float) -> "Layer":
        from .kanyu import points_along_lines as _f

        return Layer(_f(self.geojson, distance))

    def add_geometry_attributes(self) -> "Layer":
        from .kanyu import add_geometry_attributes as _f

        return Layer(_f(self.geojson))

    def mean_coordinates(self, weight: str | None = None) -> "Layer":
        from .kanyu import mean_coordinates as _f

        return Layer(_f(self.geojson, weight))

    # —— 分析 ——

    def dissolve(self, field: str | None = None) -> "Layer":
        from .kanyu import dissolve as _dissolve

        return Layer(_dissolve(self.geojson, field))

    def overlay(self, other: "Layer", operation: str) -> "Layer":
        from .kanyu import overlay as _overlay

        return Layer(_overlay(self.geojson, other.geojson, operation))

    def sjoin(self, other: "Layer", predicate: str) -> "Layer":
        from .kanyu import sjoin as _sjoin

        return Layer(_sjoin(self.geojson, other.geojson, predicate))

    def count_points_in_polygon(self, points: "Layer") -> "Layer":
        from .kanyu import count_points_in_polygon as _f

        return Layer(_f(self.geojson, points.geojson))

    def merge(self, *others: "Layer") -> "Layer":
        from .kanyu import merge as _merge

        return Layer(_merge([self.geojson, *(o.geojson for o in others)]))

    def reproject(self, from_crs: str, to_crs: str) -> "Layer":
        from .kanyu import reproject as _reproject

        return Layer(_reproject(self.geojson, from_crs, to_crs))

    # —— 属性表 ——

    def calc_field(self, target: str, expression: str) -> "Layer":
        """字段计算器：表达式写入目标字段（不存在则新建）。

        >>> layer.calc_field("area2", "$area")  # 测地面积 ㎡
        """
        from .kanyu import calc_field as _f

        return Layer(_f(self.geojson, target, expression))

    def add_field(self, name: str, default: str | None = None) -> "Layer":
        """新建字段；default 为 JSON 文本（如 "0"），None 得 Null。"""
        from .kanyu import add_field as _f

        return Layer(_f(self.geojson, name, default))

    def delete_field(self, name: str) -> "Layer":
        from .kanyu import delete_field as _f

        return Layer(_f(self.geojson, name))

    def rename_field(self, old: str, new: str) -> "Layer":
        from .kanyu import rename_field as _f

        return Layer(_f(self.geojson, old, new))

    # —— 报告（返回解析后的 dict）——

    def stats(self) -> dict:
        from .kanyu import stats as _stats

        return _json.loads(_stats(self.geojson))

    def field_stats(self, field: str) -> dict:
        from .kanyu import field_stats as _f

        return _json.loads(_f(self.geojson, field))

    def nearest_neighbor(self) -> dict:
        from .kanyu import nearest_neighbor as _f

        return _json.loads(_f(self.geojson))

    def distance_matrix(self, other: "Layer") -> dict:
        from .kanyu import distance_matrix as _f

        return _json.loads(_f(self.geojson, other.geojson))

    def measure(self, kind: str) -> dict:
        from .kanyu import measure as _f

        return _json.loads(_f(self.geojson, kind))

    def split_by_field(self, field: str) -> list[tuple[str, "Layer"]]:
        """按字段值分组：返回 [(组值, Layer), …]（BTreeMap 字典序）。"""
        from .kanyu import split_by_field as _f

        groups = _json.loads(_f(self.geojson, field))
        return [(g["key"], Layer(_json.dumps(g["collection"]))) for g in groups]

    # —— 落盘 ——

    def export(self, out: str, format: str) -> None:
        from .kanyu import export as _export

        _export(self.geojson, out, format)


__all__ = [
    "Layer",
    "add_field",
    "add_geometry_attributes",
    "boundary",
    "bounding_boxes",
    "buffer",
    "calc_field",
    "centroid",
    "concave_hull",
    "convex_hull",
    "count_points_in_polygon",
    "create_grid",
    "crs_info",
    "delete_field",
    "delete_holes",
    "dissolve",
    "distance_matrix",
    "explode",
    "export",
    "extract_by_attribute",
    "extract_by_location",
    "field_stats",
    "load",
    "mean_coordinates",
    "measure",
    "merge",
    "minimum_rotated_rect",
    "multi_ring_buffer",
    "nearest_neighbor",
    "overlay",
    "points_along_lines",
    "query",
    "rename_field",
    "render_png",
    "render_svg",
    "reproject",
    "run_tool",
    "search_crs",
    "simplify",
    "sjoin",
    "split_by_field",
    "stats",
    "toolbox_registry",
    "topology",
    "validate_crs",
    "variable_buffer",
    "version",
    "zonal_stats",
]
