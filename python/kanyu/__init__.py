"""堪舆 Kanyu Python SDK —— Rust 内核的 Python 门面。

用法::

    import kanyu
    fc = kanyu.load("buildings.geojson")
    high = kanyu.query(fc, "height > 50")
    buf = kanyu.buffer(high, 500.0)
    kanyu.export(buf, "high_buffer.kdb", "kdb")

数据契约：GeoJSON FeatureCollection 文本（函数进出均为 JSON 字符串）。
渲染函数返回 PNG 字节 / SVG 文本。统计/度量返回 JSON 文本（json.loads 后使用）。
"""

from .kanyu import (  # noqa: F401 —— 原生函数再导出（Rust 内核实现）
    buffer,
    centroid,
    convex_hull,
    delete_holes,
    dissolve,
    explode,
    export,
    load,
    measure,
    overlay,
    query,
    render_png,
    render_svg,
    reproject,
    simplify,
    sjoin,
    stats,
    topology,
    version,
    zonal_stats,
)


class Layer:
    """图层便捷封装：持有一份 GeoJSON 文本，链式调用内核函数。

    >>> layer = kanyu.Layer.load("buildings.geojson")
    >>> layer.query("height > 50").buffer(500).export("out.fgb", "fgb")
    """

    def __init__(self, geojson_text: str):
        self.geojson = geojson_text

    @classmethod
    def load(cls, path: str) -> "Layer":
        from .kanyu import load as _load

        return cls(_load(path))

    def query(self, expression: str) -> "Layer":
        from .kanyu import query as _query

        return Layer(_query(self.geojson, expression))

    def buffer(self, distance: float, segments: int = 16) -> "Layer":
        from .kanyu import buffer as _buffer

        return Layer(_buffer(self.geojson, distance, segments))

    def reproject(self, from_crs: str, to_crs: str) -> "Layer":
        from .kanyu import reproject as _reproject

        return Layer(_reproject(self.geojson, from_crs, to_crs))

    def simplify(self, tolerance: float) -> "Layer":
        from .kanyu import simplify as _simplify

        return Layer(_simplify(self.geojson, tolerance))

    def dissolve(self, field: str | None = None) -> "Layer":
        from .kanyu import dissolve as _dissolve

        return Layer(_dissolve(self.geojson, field))

    def stats(self) -> dict:
        import json

        from .kanyu import stats as _stats

        return json.loads(_stats(self.geojson))

    def export(self, out: str, format: str) -> None:
        from .kanyu import export as _export

        _export(self.geojson, out, format)


__all__ = [
    "Layer",
    "buffer",
    "centroid",
    "convex_hull",
    "delete_holes",
    "dissolve",
    "explode",
    "export",
    "load",
    "measure",
    "overlay",
    "query",
    "render_png",
    "render_svg",
    "reproject",
    "simplify",
    "sjoin",
    "stats",
    "topology",
    "version",
    "zonal_stats",
]
