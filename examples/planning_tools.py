"""堪舆示例工具箱 —— ArcGIS Pro .pyt 式样的 Python 工具箱样例。

用 Rust 内核（kanyu 扩展模块）驱动的 Python 工具：
    kanyu toolbox list examples/planning_tools.py
    kanyu toolbox run examples/planning_tools.py buffer500 --param input=examples/buildings.geojson
    kanyu toolbox run examples/planning_tools.py highrise_report --param input=examples/buildings.geojson
"""

import json

import kanyu
from kanyu.toolbox import Param, Tool, Toolbox


class 规划分析(Toolbox):
    alias = "planning"

    class Buffer500(Tool):
        name = "buffer500"
        label = "缓冲区分析"
        description = "对输入图层按距离生成缓冲区（米制请先投影）"

        params = [
            Param("input", "输入图层路径", "path"),
            Param("distance", "缓冲距离（CRS 单位）", "float", default=500.0),
            Param("out", "输出路径（可空打印 GeoJSON）", "path", optional=True),
        ]

        def execute(self, args):
            fc = kanyu.load(args["input"])
            result = json.loads(kanyu.buffer(fc, float(args["distance"])))
            if args.get("out"):
                out = args["out"]
                fmt = out.rsplit(".", 1)[-1]
                kanyu.export(json.dumps(result), out, fmt)
                return {"out": out, "feature_count": len(result["features"])}
            return result

    class HighriseReport(Tool):
        name = "highrise_report"
        label = "高层建筑报告"
        description = "筛选高于阈值的建筑并输出统计（面积/周长/亩）"

        params = [
            Param("input", "输入图层路径", "path"),
            Param("threshold", "高度阈值（米）", "float", default=50.0),
        ]

        def execute(self, args):
            fc = kanyu.load(args["input"])
            high = kanyu.query(fc, f"height > {float(args['threshold'])}")
            stats = json.loads(kanyu.stats(high))
            return {
                "threshold_m": float(args["threshold"]),
                "count": stats["feature_count"],
                "total_area_m2": stats["total_area_m2"],
                "total_area_mu": stats["total_area_mu"],
            }
